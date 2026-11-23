//! Running under the Windows service manager.
//!
//! `giams-node service` is what ops/install-service.ps1 registers. Without
//! this the service manager waits for the program to report in, gives up
//! after 30 seconds and kills it ("did not respond in a timely fashion").
//!
//! Differences from a console run:
//!   * the working folder is the exe's own, since services start in System32
//!     and .env, the keys, tls/ and backups/ live beside the exe
//!   * logs go to logs\giams-node.log there (the first-run administrator
//!     password is printed into it)
//!   * a startup failure ends the service with an error code, so the
//!     service's restart-on-failure setting tries again

use std::ffi::OsString;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use tokio::sync::Notify;
use windows_service::service::{
    ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType,
};
use windows_service::service_control_handler::{self, ServiceControlHandlerResult};
use windows_service::{define_windows_service, service_dispatcher};

// ignored for a service in its own process, but the API wants one
const NAME: &str = "GIAMS";

define_windows_service!(ffi_service_main, service_main);

pub fn run() -> anyhow::Result<()> {
    service_dispatcher::start(NAME, ffi_service_main).context(
        "`service` is only for the Windows service manager; to run the server by hand, start it without arguments",
    )
}

fn service_main(_arguments: Vec<OsString>) {
    // nowhere to report this but the exit code the service manager records
    let _ = run_service();
}

fn run_service() -> anyhow::Result<()> {
    let folder = std::env::current_exe()
        .context("could not find this program's folder")?
        .parent()
        .context("could not find this program's folder")?
        .to_path_buf();
    std::env::set_current_dir(&folder).context("could not open this program's folder")?;

    std::fs::create_dir_all("logs").ok();
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("logs/giams-node.log")
        .context("could not open logs/giams-node.log")?;
    crate::init(Some(log));

    let stop = Arc::new(Notify::new());
    let handle = {
        let stop = Arc::clone(&stop);
        service_control_handler::register(NAME, move |event| match event {
            ServiceControl::Stop | ServiceControl::Shutdown => {
                stop.notify_one();
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        })
        .context("could not register with the service manager")?
    };

    let status = |state, code: u32, wait_secs: u64| ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: state,
        controls_accepted: if state == ServiceState::Running {
            ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN
        } else {
            ServiceControlAccept::empty()
        },
        exit_code: ServiceExitCode::Win32(code),
        checkpoint: 0,
        wait_hint: Duration::from_secs(wait_secs),
        process_id: None,
    };

    handle.set_service_status(status(ServiceState::Running, 0, 0))?;
    tracing::info!(folder = %folder.display(), "running as a Windows service");

    let outcome = tokio::runtime::Runtime::new()
        .context("could not start the server's runtime")
        .and_then(|runtime| {
            runtime.block_on(async {
                tokio::select! {
                    result = crate::run() => result,
                    _ = stop.notified() => {
                        tracing::info!("stopping at the service manager's request");
                        Ok(())
                    }
                }
            })
        });

    // a database transaction is atomic, so dropping requests in flight never
    // leaves half a record; the browser or desktop app just retries
    let code = match &outcome {
        Ok(()) => 0,
        Err(e) => {
            tracing::error!(error = ?e, "the server stopped");
            1
        }
    };
    handle.set_service_status(status(ServiceState::Stopped, code, 0))?;
    outcome
}
