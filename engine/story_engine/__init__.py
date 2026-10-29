"""story_engine — deterministic 2D animation engine for Story Animator.

The orchestrator (Rust) writes one JSON *render spec* per scene; this package
turns each spec into an MP4 clip using Manim (Cairo renderer). Nothing in here
calls an LLM or the network: the same spec always renders the same clip.
"""

from .fonts import configure_fonts

# Must happen before anything imports manim / manimpango.
configure_fonts()

__version__ = "2.0.0"
