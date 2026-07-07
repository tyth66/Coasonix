pub mod artifact;
pub mod canonical;
pub mod kernel;
pub mod policy;
pub mod replay;
pub mod sandbox;
pub mod schema;
pub mod state;
pub mod storage;

#[cfg(test)]
mod tests {
    #[test]
    fn runtime_core_smoke_test_runs() {
        assert_eq!(2 + 2, 4);
    }
}
