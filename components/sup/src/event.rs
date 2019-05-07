// Copyright (c) 2019 Chef Software Inc. and/or applicable contributors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Main interface for a stream of events the Supervisor can send out
//! in the course of its operations.
//!
//! Currently, the Supervisor is able to send events to a [NATS
//! Streaming][1] server. The `init_stream` function must be called
//! before sending events to initialize the publishing thread in the
//! background. Thereafter, you can pass "event" structs to the
//! `event` function, which will publish the event to the stream.
//!
//! All events are published under the "habitat" subject.
//!
//! [1]:https://github.com/nats-io/nats-streaming-server

mod error;

#[cfg(feature = "nitox_stream")]
#[path = "event/nitox.rs"]
mod stream_impl;

// This is the default, but setting it as a default in Cargo.toml
// makes it trickier to use nitox instead.
#[cfg(any(feature = "ratsio_stream", not(feature = "nitox_stream")))]
#[path = "event/ratsio.rs"]
mod stream_impl;
mod types;

use self::types::{EventMessage,
                  EventMetadata,
                  HealthCheckEvent,
                  ServiceStartedEvent,
                  ServiceStoppedEvent};
use crate::{error::Result as SupResult,
            manager::{service::{HealthCheck,
                                Service},
                      sys::Sys}};
use clap::ArgMatches;
pub use error::{Error,
                Result};
use futures::sync::mpsc::UnboundedSender;
use habitat_common::types::{AutomateAuthToken,
                            EventStreamMetadata};
use habitat_core::env::Config as EnvConfig;
use state::Container;
use std::{net::SocketAddr,
          num::ParseIntError,
          str::FromStr,
          sync::Once,
          time::Duration};

static INIT: Once = Once::new();
lazy_static! {
    // TODO (CM): When const fn support lands in stable, we can ditch
    // this lazy_static call.

    /// Reference to the event stream.
    static ref EVENT_STREAM: Container = Container::new();
    /// Core information that is shared between all events.
    static ref EVENT_CORE: Container = Container::new();
}

/// Starts a new thread for sending events to a NATS Streaming
/// server. Stashes the handle to the stream, as well as the core
/// event information that will be a part of all events, in a global
/// static reference for access later.
pub fn init_stream(config: EventStreamConfig, event_core: EventCore) -> Result<()> {
    // call_once can't return a Result (or anything), so we'll fake it
    // by hanging onto any error here.
    let mut init_err: Option<Error> = None;

    INIT.call_once(|| {
            let conn_info = EventConnectionInfo::new(config.token, config.url);
            match stream_impl::init_stream(conn_info) {
                Ok(event_stream) => {
                    EVENT_STREAM.set(event_stream);
                    EVENT_CORE.set(event_core);
                }
                Err(e) => init_err = Some(e),
            }
        });

    if let Some(err) = init_err {
        Err(err)
    } else {
        Ok(())
    }
}

/// Captures all event stream-related configuration options that would
/// be passed in by a user
#[derive(Clone, Debug)]
pub struct EventStreamConfig {
    environment: String,
    application: String,
    meta:        EventStreamMetadata,
    token:       AutomateAuthToken,
    url:         String,
}

impl EventStreamConfig {
    /// Create an instance from Clap arguments.
    // TODO (CM): result type!
    pub fn from_matches(m: &ArgMatches) -> SupResult<EventStreamConfig> {
        Ok(EventStreamConfig { environment: m.value_of("EVENT_STREAM_ENVIRONMENT")
                                             .map(str::to_string)
                                             .expect("Required option for EventStream feature"),
                               application: m.value_of("EVENT_STREAM_APPLICATION")
                                             .map(str::to_string)
                                             .expect("Required option for EventStream feature"),
                               meta:        EventStreamMetadata::from_matches(m)?,
                               token:       AutomateAuthToken::from_matches(m)?,
                               url:         m.value_of("EVENT_STREAM_URL")
                                             .map(str::to_string)
                                             .expect("Required option for EventStream feature"), })
    }
}

/// All the information needed to establish a connection to a NATS
/// Streaming server.
// TODO: This will change as we firm up what the interaction between
// Habitat and A2 looks like.
pub struct EventConnectionInfo {
    pub name:        String,
    pub verbose:     bool,
    pub cluster_uri: String,
    pub cluster_id:  String,
    pub auth_token:  AutomateAuthToken,
}

impl EventConnectionInfo {
    pub fn new(auth_token: AutomateAuthToken, cluster_uri: String) -> Self {
        EventConnectionInfo { name: String::from("hab_client"),
                              verbose: true,
                              cluster_uri,
                              cluster_id: String::from("event-service"),
                              auth_token }
    }
}

/// A collection of data that will be present in all events. Rather
/// than baking this into the structure of each event, we represent it
/// once and merge the information into the final rendered form of the
/// event.
///
/// This prevents us from having to thread information throughout the
/// system, just to get it to the places where the events are
/// generated (e.g., not all code has direct access to the
/// Supervisor's ID).
#[derive(Clone, Debug)]
pub struct EventCore {
    /// The unique identifier of the Supervisor sending the event.
    supervisor_id: String,
    ip_address:    SocketAddr,
    fqdn:          String,
    // TODO (CM): could add application and environment to the meta
    // map directly... hrmm
    application: String,
    environment: String,
    meta:        EventStreamMetadata,
}

impl EventCore {
    pub fn new(config: &EventStreamConfig, sys: &Sys) -> Self {
        EventCore { supervisor_id: sys.member_id.clone(),
                    ip_address:    sys.gossip_listen(),
                    fqdn:          sys.hostname.clone(),
                    environment:   config.environment.clone(),
                    application:   config.application.clone(),
                    meta:          config.meta.clone(), }
    }
}

/// Send an event for the start of a Service.
pub fn service_started(service: &Service) {
    if stream_initialized() {
        publish(ServiceStartedEvent { service_metadata: Some(service.to_service_metadata()),
                                      event_metadata:   None, });
    }
}

/// Send an event for the stop of a Service.
pub fn service_stopped(service: &Service) {
    if stream_initialized() {
        publish(ServiceStoppedEvent { service_metadata: Some(service.to_service_metadata()),
                                      event_metadata:   None, });
    }
}

pub fn health_check(service: &Service,
                    check_result: HealthCheck,
                    duration: Duration,
                    has_hook: bool) {
    if stream_initialized() {
        publish(HealthCheckEvent { service_metadata: Some(service.to_service_metadata()),
                                   event_metadata: None,
                                   result: Into::<types::HealthCheck>::into(check_result)
                                           as i32,
                                   duration: Some(duration.into()),
                                   has_hook });
    }
}

////////////////////////////////////////////////////////////////////////

/// Internal helper function to know whether or not to go to the trouble of
/// creating event structures. If the event stream hasn't been
/// initialized, then we shouldn't need to do anything.
fn stream_initialized() -> bool { EVENT_STREAM.try_get::<EventStream>().is_some() }

/// Publish an event. This is the main interface that client code will
/// use.
///
/// If `init_stream` has not been called already, this function will
/// be a no-op.
fn publish(mut event: impl EventMessage) {
    if let Some(e) = EVENT_STREAM.try_get::<EventStream>() {
        // TODO (CM): Yeah... this is looking pretty gross. The
        // intention is to be able to timestamp the events right as
        // they go out.
        //
        // We *could* set the time when we convert the EventCore to a
        // EventMetadata struct, but that seems odd.
        //
        // It probably means that this structure just isn't the right
        // one.
        //
        // The ugliness is at least contained, though.
        event.event_metadata(EventMetadata { timestamp:
                                                 Some(std::time::SystemTime::now().into()),
                                             ..EVENT_CORE.get::<EventCore>().to_event_metadata() });

        e.send(event.to_bytes());
    }
}

/// A lightweight handle for the event stream. All events get to the
/// event stream through this.
struct EventStream(UnboundedSender<Vec<u8>>);

impl EventStream {
    /// Queues an event to be sent out.
    fn send(&self, event: Vec<u8>) {
        trace!("About to queue an event: {:?}", event);
        if let Err(e) = self.0.unbounded_send(event) {
            error!("Failed to queue event: {:?}", e);
        }
    }
}

////////////////////////////////////////////////////////////////////////

/// How long should we for the event thread to start up before
/// abandoning it and shutting down?
#[derive(Clone, Debug)]
struct EventThreadStartupWait(u64);

impl Default for EventThreadStartupWait {
    fn default() -> Self { Self(5) }
}

impl FromStr for EventThreadStartupWait {
    type Err = ParseIntError;

    fn from_str(s: &str) -> ::std::result::Result<Self, Self::Err> { Ok(Self(s.parse()?)) }
}

impl EnvConfig for EventThreadStartupWait {
    const ENVVAR: &'static str = "HAB_EVENT_THREAD_STARTUP_WAIT_SEC";
}

impl Into<Duration> for EventThreadStartupWait {
    fn into(self) -> Duration { Duration::from_secs(self.0) }
}
