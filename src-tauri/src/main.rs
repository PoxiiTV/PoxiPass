// Sin consola en release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // El navegador lanza el exe para hablar con la extensión: modo relé, sin ventana ni WebView
    if poxipass_lib::relay::is_relay() {
        return poxipass_lib::relay::run();
    }
    poxipass_lib::run()
}
