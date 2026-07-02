pub mod canonical;
pub mod schema;

#[cfg(test)]
mod tests {
    #[test]
    fn runtime_core_smoke_test_runs() {
        assert_eq!(2 + 2, 4);
    }
}
