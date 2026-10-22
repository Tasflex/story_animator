"""
Python bindings for the mapbox earcut C++ polygon triangulation library.

This module provides fast triangulation of 2D polygons using the Mapbox Earcut algorithm.
"""


# start delvewheel patch
def _delvewheel_patch_1_13_0():
    import os
    if os.path.isdir(libs_dir := os.path.abspath(os.path.join(os.path.dirname(__file__), os.pardir, 'mapbox_earcut.libs'))):
        os.add_dll_directory(libs_dir)


_delvewheel_patch_1_13_0()
del _delvewheel_patch_1_13_0
# end delvewheel patch

# Import all functions from the compiled extension module
from ._core import (
    triangulate_float32,
    triangulate_float64,
    triangulate_int32,
    triangulate_int64,
    __version__,
)

__all__ = [
    "triangulate_float32",
    "triangulate_float64",
    "triangulate_int32",
    "triangulate_int64",
    "__version__",
]
