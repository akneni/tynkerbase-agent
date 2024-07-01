const fn get_proj_path() -> &'static str {
    // Allows developers to build and test in their local project directory
    // rather than directly in /usr/share

    #[allow(unused_assignments)]
    let mut root_dir = "/usr/share/tynkerbase-agent";
    #[cfg(debug_assertions)] {
        root_dir = ".";
    }
    root_dir
}

pub const AGENT_ROOTDIR_PATH: &str = get_proj_path();
