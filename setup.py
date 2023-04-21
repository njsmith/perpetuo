from setuptools import setup
from setuptools_rust import Binding, RustBin, RustExtension, Strip

setup(
    name="perpetuo",
    rust_extensions=[
        RustExtension(
            "perpetuo._perpetuo",
            binding=Binding.PyO3,
            py_limited_api=True,
            # strip=Strip.Debug
        ),
        RustBin("perpetuo", strip=Strip.Debug),
    ],
    package_dir={"": "python"},
    packages=["perpetuo"],
    package_data={"perpetuo": ["py.typed", "__init__.pyi"]},
    zip_safe=False,
)
