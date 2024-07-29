const fn get_proj_path() -> &'static str {
    // Allows developers to build and test in their local project directory
    // rather than directly in /usr/share

    #[cfg(debug_assertions)]
    {
        return ".";
    }
    #[allow(unreachable_code)]
    "/usr/share/tynkerbase-agent"
}

pub const AGENT_ROOTDIR_PATH: &str = get_proj_path();
pub const SERVER_ENDPOINT: &str = "https://tynkerbase-server.shuttleapp.rs";
pub const CONTAINER_MOD: &str = "__tyb_container";
pub const IMAGE_MOD: &str = "__tyb_image";