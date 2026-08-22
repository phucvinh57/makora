pub trait IgnorePolicy: Send + Sync {
    fn is_app_ignored(&self, exe_path: &str) -> bool;
    fn is_site_ignored(&self, url: &str) -> bool;
}

#[cfg(test)]
pub struct NoopIgnorePolicy;

#[cfg(test)]
impl IgnorePolicy for NoopIgnorePolicy {
    fn is_app_ignored(&self, _exe_path: &str) -> bool {
        false
    }

    fn is_site_ignored(&self, _url: &str) -> bool {
        false
    }
}
