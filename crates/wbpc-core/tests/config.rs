use std::{net::SocketAddr, path::Path};
use wbpc_core::Config;

#[test]
fn defaults_are_private_and_relative_to_the_binary() {
    let home = Path::new("app");
    let c = Config::from_pairs([], home).unwrap();
    assert_eq!(c.addr, "127.0.0.1:3040".parse::<SocketAddr>().unwrap());
    assert_eq!(c.access_dir, home.join("access_account"));
    assert_eq!(c.data_dir, home.join("data"));
    assert_eq!(c.redis_url.as_str(), "redis://127.0.0.1:6379/0");
}

#[test]
fn invalid_values_fail_without_echoing_secrets() {
    let result = Config::from_pairs(
        [("WBP_REDIS_URL", "https://secret@example.com")],
        Path::new("."),
    );
    assert!(!result.err().unwrap().to_string().contains("secret"));
    assert!(Config::from_pairs([("WBP_ADDR", "broken")], Path::new(".")).is_err());
}

#[test]
fn a_separate_redis_password_is_url_encoded() {
    let c = Config::from_pairs([("WBP_REDIS_PASSWORD", "ab@/#")], Path::new(".")).unwrap();
    assert_eq!(c.redis_url.password(), Some("ab%40%2F%23"));
}
