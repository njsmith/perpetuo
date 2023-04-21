pub mod shmem;

use crate::shmem::{alloc_slot, StallTracker};
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;

#[pyclass(name = "StallTracker", module = "perpetuo")]
struct PyStallTracker {
    stall_tracker: &'static mut StallTracker,
}

#[derive(FromPyObject)]
enum ThreadHint {
    #[pyo3(transparent, annotation = "str")]
    String(String),
    #[pyo3(transparent, annotation = "int")]
    Int(usize),
}

impl ThreadHint {
    fn encode(&self) -> PyResult<usize> {
        match self {
            ThreadHint::String(s) => {
                if s == "gil" {
                    Ok(0)
                } else {
                    Err(PyValueError::new_err("must be integer or 'gil'"))
                }
            }
            ThreadHint::Int(i) => Ok(*i),
        }
    }
}

#[pymethods]
impl PyStallTracker {
    #[new]
    fn new(name: &str, thread_hint: ThreadHint) -> PyResult<Self> {
        let stall_tracker = match alloc_slot(name, thread_hint.encode()?) {
            Ok(slot) => slot,
            Err(err) => return Err(PyRuntimeError::new_err(err.to_string())),
        };
        Ok(PyStallTracker { stall_tracker })
    }

    fn go_active(&self) -> PyResult<()> {
        if self.stall_tracker.is_active() {
            return Err(PyRuntimeError::new_err("Already active"));
        }
        self.stall_tracker.toggle();
        Ok(())
    }

    fn go_idle(&self) -> PyResult<()> {
        if !self.stall_tracker.is_active() {
            return Err(PyRuntimeError::new_err("Already idle"));
        }
        self.stall_tracker.toggle();
        Ok(())
    }

    fn is_active(&self) -> bool {
        self.stall_tracker.is_active()
    }

    fn counter_address(&self) -> usize {
        &self.stall_tracker.count as *const _ as usize
    }
}

/// Same as time.sleep, but it holds the GIL. Useful for testing.
#[pyfunction]
fn _stall_gil(seconds: f64) {
    std::thread::sleep(std::time::Duration::from_secs_f64(seconds));
}

#[pymodule]
fn _perpetuo(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<PyStallTracker>()?;
    m.add_function(wrap_pyfunction!(_stall_gil, m)?)?;
    Ok(())
}
