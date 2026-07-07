use pyo3::prelude::*;

/// PyO3 bindings for csearch-core
#[pymodule]
fn csearch(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add("__version__", "0.1.0")?;

    // TODO: Expose CodeBase, Match, Location classes

    Ok(())
}
