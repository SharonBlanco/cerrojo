// En Windows, el icono dentro de cerrojo.exe (Explorador y barra de tareas).
fn main() {
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("packaging/cerrojo.ico");
        res.set("FileDescription", "Cerrojo");
        res.set("ProductName", "Cerrojo");
        let _ = res.compile();
    }
}
