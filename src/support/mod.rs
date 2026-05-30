
//! Various runtimes for hyper
pub mod iroh;
pub mod connection_pool;


pub fn get_value_from_env<T : Sized + std::str::FromStr>(key: &str) -> Option<T> {
    if let Ok(val) = std::env::var(key) {
        if let Ok(val) = val.parse::<T>() {
            return Some(val);
        }
    }
    None
}