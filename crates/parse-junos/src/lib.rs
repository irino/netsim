pub mod parser;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod golden_tests;

pub use parser::parse_junos_config;
