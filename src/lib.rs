pub mod shmem;
pub mod log;

use crate::shmem::{alloc_slot, release_slot, StallTracker, ThreadHint, GIL};
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;

#[pyclass(name = "StallTracker", module = "perpetuo")]
struct PyStallTracker {
    stall_tracker: Option<&'static mut StallTracker>,
}

#[derive(FromPyObject)]
enum ThreadHintArg {
    #[pyo3(transparent, annotation = "str")]
    String(String),
    #[pyo3(transparent, annotation = "int")]
    Int(usize),
}

impl ThreadHintArg {
    fn encode(&self) -> PyResult<ThreadHint> {
        match self {
            ThreadHintArg::String(s) => {
                if s == "gil" {
                    Ok(GIL)
                } else {
                    Err(PyValueError::new_err("must be integer or the string 'gil'"))
                }
            }
            ThreadHintArg::Int(i) => match ThreadHint::from_thread_id(*i) {
                Ok(thread_hint) => Ok(thread_hint),
                Err(rust_err) => Err(PyValueError::new_err(rust_err.to_string())),
            },
        }
    }
}

fn rustify(py: &PyStallTracker) -> PyResult<&&mut StallTracker> {
    py.stall_tracker
        .as_ref()
        .ok_or_else(|| PyRuntimeError::new_err("attempt to use closed StallTracker"))
}

#[pymethods]
impl PyStallTracker {
    #[new]
    fn new(name: &str, thread_hint: ThreadHintArg) -> PyResult<Self> {
        let stall_tracker = match alloc_slot(name, thread_hint.encode()?) {
            Ok(slot) => slot,
            Err(err) => return Err(PyRuntimeError::new_err(err.to_string())),
        };
        Ok(PyStallTracker {
            stall_tracker: Some(stall_tracker),
        })
    }

    fn _leak(this: PyRef<'_, Self>, py: Python) {
        std::mem::forget(this.into_py(py));
    }

    fn go_active(&self) -> PyResult<()> {
        let stall_tracker = rustify(&self)?;
        if stall_tracker.is_active() {
            return Err(PyRuntimeError::new_err("Already active"));
        }
        stall_tracker.toggle();
        Ok(())
    }

    fn go_idle(&self) -> PyResult<()> {
        let stall_tracker = rustify(&self)?;
        if !stall_tracker.is_active() {
            return Err(PyRuntimeError::new_err("Already idle"));
        }
        stall_tracker.toggle();
        Ok(())
    }

    fn is_active(&self) -> PyResult<bool> {
        let stall_tracker = rustify(&self)?;
        Ok(stall_tracker.is_active())
    }

    fn counter_address(&self) -> PyResult<usize> {
        Ok(&rustify(&self)?.count as *const _ as usize)
    }

    fn close(&mut self) -> PyResult<()> {
        if let Some(stall_tracker) = self.stall_tracker.take() {
            release_slot(stall_tracker).map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
        }
        Ok(())
    }
}

impl Drop for PyStallTracker {
    fn drop(&mut self) {
        if let Err(err) = self.close() {
            eprintln!("Warning: unraiseable error in perpetuo library: {err}");
        }
    }
}

/// Same as time.sleep, but it holds the GIL. Useful for testing.
#[pyfunction]
fn stall_gil(seconds: f64) {
    std::thread::sleep(std::time::Duration::from_secs_f64(seconds));
}

#[pymodule]
fn _perpetuo(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<PyStallTracker>()?;
    m.add_function(wrap_pyfunction!(stall_gil, m)?)?;
    Ok(())
}
