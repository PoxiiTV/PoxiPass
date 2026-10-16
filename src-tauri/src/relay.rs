//! Modo relé: el navegador lanza `PoxiPass.exe chrome-extension://<id>/` para hablar con la extensión por
//! stdin/stdout (Native Messaging). Este proceso no arranca la app ni carga WebView: pasa los bytes tal cual a la
//! app abierta por la tubería del puente (`bridge.rs`), que es quien valida y responde.

use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient};

pub fn is_relay() -> bool {
    std::env::args().nth(1).is_some_and(|a| a.starts_with("chrome-extension://"))
}

pub fn run() {
    let Ok(rt) = tokio::runtime::Builder::new_current_thread().enable_all().build() else { return };
    rt.block_on(async {
        let Some(pipe) = connect().await else { return };
        let (mut pr, mut pw) = tokio::io::split(pipe);
        // Cuando un lado se cierra se acaba todo (el navegador cierra stdin al desconectar)
        tokio::select! {
            _ = pump(tokio::io::stdin(), &mut pw) => {},
            _ = pump(&mut pr, tokio::io::stdout()) => {},
        }
    });
    // La lectura de stdin vive en un hilo bloqueante que no termina solo: se sale sin esperarlo
    std::process::exit(0);
}

async fn pump(mut from: impl tokio::io::AsyncRead + Unpin, mut to: impl tokio::io::AsyncWrite + Unpin) {
    let mut buf = vec![0u8; 16 * 1024];
    while let Ok(n) = from.read(&mut buf).await {
        if n == 0 || to.write_all(&buf[..n]).await.is_err() || to.flush().await.is_err() {
            return;
        }
    }
}

/// Conecta con la app; si no está abierta, la arranca en segundo plano y espera a que el puente exista.
async fn connect() -> Option<NamedPipeClient> {
    let name = crate::bridge::pipe_name();
    let mut launched = false;
    for _ in 0..150 {
        match ClientOptions::new().open(&name) {
            // Otro proceso podría haber creado una tubería con ese nombre: solo se habla con nuestro mismo exe
            Ok(c) => return crate::bridge::is_our_exe(crate::bridge::server_pid(&c)?).then_some(c),
            Err(e) if e.raw_os_error() == Some(windows_sys::Win32::Foundation::ERROR_PIPE_BUSY as i32) => {}
            Err(_) if !launched => {
                launched = true;
                launch_app();
            }
            Err(_) => {}
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    None
}

fn launch_app() {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
    let Ok(exe) = std::env::current_exe() else { return };
    let spawn = |flags: u32| Command::new(&exe).arg("--hidden").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).creation_flags(flags).spawn();
    // Fuera del job del navegador, para que la app no se cierre con él; si el job no lo permite, se lanza igual
    if spawn(DETACHED_PROCESS | CREATE_BREAKAWAY_FROM_JOB).is_err() {
        let _ = spawn(DETACHED_PROCESS);
    }
}
