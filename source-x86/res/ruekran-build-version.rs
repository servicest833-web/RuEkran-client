pub fn apply(resource: &mut winres::WindowsResource) {
    let numeric = include_str!("ruekran-version-numeric.txt").trim();
    let mut packed = 0u64;
    for part in numeric.split('.') {
        let component = part.parse::<u16>().expect("Invalid generated RuEkran version");
        packed = (packed << 16) | u64::from(component);
    }
    resource
        .set_version_info(winres::VersionInfo::FILEVERSION, packed)
        .set_version_info(winres::VersionInfo::PRODUCTVERSION, packed)
        .set("FileVersion", numeric)
        .set("ProductVersion", include_str!("ruekran-version.txt").trim());
}
