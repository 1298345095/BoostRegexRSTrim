# Establish performance baselines before setting regression thresholds

Establish performance baselines for the trimmed C++ reference and the Rust implementation before agreeing on acceptable regression thresholds. No percentage or performance acceptance result is assumed yet. Measure compilation, matching, replacement, and memory use, and record the measurement environment and conditions.

Run functional compatibility comparisons for all six target combinations on GitHub Actions. Performance acceptance initially covers the user's designated primary runtime environments; its matrix need not match all six functional targets. The user will designate performance test machines later.

Measure both engines on the same designated machine with identical inputs and comparable build conditions. Do not directly compare absolute timings from different machines or treat GitHub Actions functional validation as performance acceptance.

Other agents will develop the repository on GitHub. Performance measurements may run only on user-designated machines; performance data from the computer used for the planning conversation must not be used.
