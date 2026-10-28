"""ResearchPilot local document-processing worker."""

__version__ = "0.3.0"

# Bumped whenever the stdin/stdout message contract changes.
PROTOCOL_VERSION = 1

# Bumped whenever the deterministic extraction rules change, so cached
# results produced by older rules can be detected and recomputed.
RULESET_VERSION = "rules-v1"
