//! Builder and newtype patterns for API clarity.

#[derive(Default)]
struct Config {
    host: String,
    port: u16,
}

impl Config {
    fn builder() -> ConfigBuilder {
        ConfigBuilder::default()
    }
}

#[derive(Default)]
struct ConfigBuilder {
    host: Option<String>,
    port: Option<u16>,
}

impl ConfigBuilder {
    fn host(mut self, host: &str) -> Self {
        self.host = Some(host.to_string());
        self
    }

    fn port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }

    fn build(self) -> Config {
        Config {
            host: self.host.unwrap_or_else(|| "127.0.0.1".to_string()),
            port: self.port.unwrap_or(8080),
        }
    }
}

struct UserId(String);

fn greet(user: UserId) -> String {
    format!("hello {}", user.0)
}

pub fn api_design_demo() -> String {
    let cfg = Config::builder().host("0.0.0.0").port(3000).build();
    let greeting = greet(UserId("alice".into()));

    let lines = vec![
        format!("config -> {}:{}", cfg.host, cfg.port),
        format!("newtype greeting: {greeting}"),
        String::from("Prefer builders for extensibility and newtypes for type safety"),
    ];

    lines.join("\n")
}

