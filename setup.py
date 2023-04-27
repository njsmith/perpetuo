from setuptools import setup, find_packages
from setuptools_rust import Binding, RustBin, RustExtension, Strip
from wheel.bdist_wheel import bdist_wheel


# https://github.com/joerick/python-abi3-package-sample/blob/main/setup.py
class bdist_wheel_abi3(bdist_wheel):
    def get_tag(self):
        python, abi, plat = super().get_tag()

        if python.startswith("cp"):
            # on CPython, our wheels are abi3 and compatible back to 3.9
            return "cp39", "abi3", plat

        return python, abi, plat


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
    cmdclass={"bdist_wheel": bdist_wheel_abi3},
    package_dir={"": "python"},
    packages=find_packages(where="python"),
    package_data={"perpetuo": ["py.typed", "__init__.pyi"]},
    zip_safe=False,
)
