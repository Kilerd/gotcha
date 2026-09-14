#![cfg(unix)]

use gotcha::{ConfigWrapper, GotchaApp, GotchaContext, GotchaResult, GotchaRouter, ServerConfig};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct SignalApp(PathBuf);
impl GotchaApp for SignalApp {
    type State = ();
    type Config = ();
    fn logger(&self) -> GotchaResult<()> {
        Ok(())
    }
    async fn config(&self) -> GotchaResult<ConfigWrapper<()>> {
        Ok(ConfigWrapper {
            app: (),
            server: ServerConfig {
                host: "127.0.0.1".into(),
                port: 0,
            },
        })
    }
    async fn state(&self, config: &ConfigWrapper<()>) -> GotchaResult<()> {
        std::fs::write(&self.0, format!("{}:{}", config.server.host, config.server.port)).unwrap();
        Ok(())
    }
    fn routes(&self, router: GotchaRouter<GotchaContext<(), ()>>) -> GotchaRouter<GotchaContext<(), ()>> {
        router.get("/ready", || async { "ready" })
    }
}

struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn default_signals_shutdown_cleanly() {
    const READY_FILE: &str = "GOTCHA_TEST_SHUTDOWN_READY_FILE";
    if let Some(path) = std::env::var_os(READY_FILE) {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(SignalApp(path.into()).run())
            .unwrap();
        return;
    }
    for signal in ["-INT", "-TERM"] {
        let dir = tempfile::tempdir().unwrap();
        let ready = dir.path().join("address");
        let mut process = Process(
            Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "default_signals_shutdown_cleanly", "--nocapture"])
                .env(READY_FILE, &ready)
                .stdout(Stdio::null())
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        let address = loop {
            if let Ok(address) = std::fs::read_to_string(&ready) {
                if !address.is_empty() {
                    break address;
                }
            }
            assert!(Instant::now() < deadline, "child did not initialize");
            assert!(process.0.try_wait().unwrap().is_none(), "child exited during initialization");
            std::thread::sleep(Duration::from_millis(10));
        };
        // An HTTP round-trip ensures serving and default signal handling have been polled.
        let mut stream = std::net::TcpStream::connect(address).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        stream.write_all(b"GET /ready HTTP/1.0\r\nHost: localhost\r\n\r\n").unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        assert!(response.ends_with("ready"));
        assert!(Command::new("kill").args([signal, &process.0.id().to_string()]).status().unwrap().success());
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = process.0.try_wait().unwrap() {
                assert!(status.success(), "{signal} must return from run() normally: {status}");
                break;
            }
            assert!(Instant::now() < deadline, "child did not stop after {signal}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
