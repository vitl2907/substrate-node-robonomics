///////////////////////////////////////////////////////////////////////////////
//
//  Copyright 2018-2019 Airalab <research@aira.life> 
//
//  Licensed under the Apache License, Version 2.0 (the "License");
//  you may not use this file except in compliance with the License.
//  You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
//  Unless required by applicable law or agreed to in writing, software
//  distributed under the License is distributed on an "AS IS" BASIS,
//  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
//  See the License for the specific language governing permissions and
//  limitations under the License.
//
///////////////////////////////////////////////////////////////////////////////
//! Console line interface.

use log::info;
use std::cell::RefCell;
use futures::future;
use futures::sync::oneshot;
use tokio::prelude::Future;
use tokio::runtime::{Builder as RuntimeBuilder, Runtime};
use substrate_cli::{informant, parse_and_prepare, NoCustom, ParseAndPrepare};
use substrate_service::{AbstractService, Roles as ServiceRoles};
pub use substrate_cli::{VersionInfo, IntoExit, error};

mod chain_spec;
#[macro_use]
mod service;

/// Parse command line arguments into service configuration.
pub fn run<I, T, E>(args: I, exit: E, version: VersionInfo) -> error::Result<()> where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
    E: IntoExit,
{
    match parse_and_prepare::<NoCustom, NoCustom, _>(&version, "robonomics-node", args) {
        ParseAndPrepare::Run(cmd) => cmd.run::<(), _, _, _, _>(load_spec, exit,
        |exit, _cli_args, _custom_args, config| {
            info!("{}", version.name);
            info!("  version {}", config.full_version());
            info!("  by {}, 2018, 2019", version.author);
            info!("Chain specification: {}", config.chain_spec.name());
            info!("Node name: {}", config.name);
            info!("Roles: {:?}", config.roles);
            let runtime = RuntimeBuilder::new().name_prefix("main-tokio-").build()
                .map_err(|e| format!("{:?}", e))?;
            match config.roles {
                ServiceRoles::LIGHT => run_until_exit(
                    runtime,
                    service::new_light(config).map_err(|e| format!("{:?}", e))?,
                    exit
                ),
                _ => run_until_exit(
                    runtime,
                    service::new_full(config).map_err(|e| format!("{:?}", e))?,
                    exit
                ),
            }.map_err(|e| format!("{:?}", e))
        }),
        ParseAndPrepare::BuildSpec(cmd) => cmd.run(load_spec),
        ParseAndPrepare::ExportBlocks(cmd) => cmd.run_with_builder::<(), _, _, _, _, _>(|config|
            Ok(new_full_start!(config).0), load_spec, exit),
        ParseAndPrepare::ImportBlocks(cmd) => cmd.run_with_builder::<(), _, _, _, _, _>(|config|
            Ok(new_full_start!(config).0), load_spec, exit),
        ParseAndPrepare::PurgeChain(cmd) => cmd.run(load_spec),
        ParseAndPrepare::RevertChain(cmd) => cmd.run_with_builder::<(), _, _, _, _>(|config|
            Ok(new_full_start!(config).0), load_spec),
        ParseAndPrepare::CustomCommand(_) => Ok(()),
    }
}

fn load_spec(id: &str) -> Result<Option<chain_spec::ChainSpec>, String> {
    Ok(match chain_spec::ChainOpt::from(id) {
        Some(spec) => Some(spec.load()?),
        None => None,
    })
}

fn run_until_exit<T, E>(
    mut runtime: Runtime,
    service: T,
    e: E,
) -> error::Result<()>
    where
        T: AbstractService,
        E: IntoExit,
{
    let (exit_send, exit) = exit_future::signal();

    let informant = informant::build(&service);
    runtime.executor().spawn(exit.until(informant).map(|_| ()));

    // we eagerly drop the service so that the internal exit future is fired,
    // but we need to keep holding a reference to the global telemetry guard
    let _telemetry = service.telemetry();

    let service_res = {
        let exit = e.into_exit().map_err(|_| error::Error::Other("Exit future failed.".into()));
        let service = service.map_err(|err| error::Error::Service(err));
        let select = service.select(exit).map(|_| ()).map_err(|(err, _)| err);
        runtime.block_on(select)
    };

    exit_send.fire();

    // TODO [andre]: timeout this future #1318
    let _ = runtime.shutdown_on_idle().wait();

    service_res
}

// handles ctrl-c
pub struct Exit;
impl IntoExit for Exit {
    type Exit = future::MapErr<oneshot::Receiver<()>, fn(oneshot::Canceled) -> ()>;
    fn into_exit(self) -> Self::Exit {
        // can't use signal directly here because CtrlC takes only `Fn`.
        let (exit_send, exit) = oneshot::channel();

        let exit_send_cell = RefCell::new(Some(exit_send));
        ctrlc::set_handler(move || {
            if let Some(exit_send) = exit_send_cell.try_borrow_mut().expect("signal handler not reentrant; qed").take() {
                exit_send.send(()).expect("Error sending exit notification");
            }
        }).expect("Error setting Ctrl-C handler");

        exit.map_err(drop)
    }
}
