fn main() {
    #[cfg(windows)]
    {
        let mut resource = winres::WindowsResource::new();
        resource.set_icon("assets/eframe_icon.ico");
        resource
            .compile()
            .expect("failed to embed the Emulator Hub application icon");
    }
}
