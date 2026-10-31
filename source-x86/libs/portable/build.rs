fn main() {
    #[cfg(windows)]
    {
        use std::io::Write;
        let mut res = winres::WindowsResource::new();
        mod release_version {
            include!("../../res/ruekran-build-version.rs");
        }
        res.set("ProductName", "RuЭкран")
            .set("CompanyName", "RuЭкран")
            .set("FileDescription", "RuЭкран — удалённый доступ")
            .set("OriginalFilename", "RuEkran-Windows-x86-x64.exe")
            .set("LegalCopyright", "Based on RustDesk; Copyright Purslane Tech Pte. Ltd. and contributors")
            .set_icon("../../res/icon.ico")
            .set_language(winapi::um::winnt::MAKELANGID(
                winapi::um::winnt::LANG_ENGLISH,
                winapi::um::winnt::SUBLANG_ENGLISH_US,
            ))
            .set_manifest_file("../../res/portable-manifest.xml");
        release_version::apply(&mut res);
        match res.compile() {
            Err(e) => {
                write!(std::io::stderr(), "{}", e).unwrap();
                std::process::exit(1);
            }
            Ok(_) => {}
        }
    }
}
