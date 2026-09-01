# Phase 3: Examples and Tests

1. `--no-default-features --lib --tests` runs core/lui tests without resolving Polars or egui.
2. `--features polars` runs Polars suites and variadic registry coverage without egui tests.
3. `--features egui` runs the shortcut conversion without Polars suites.
4. Default features run the committed registry freshness comparison and all optional suites.
5. Each feature group has an anchor command, so accidental disappearance fails without a brittle
   total-count floor.

Run the six configurations and `bash scripts/check-build-matrix.sh`; expected counts are recorded in
the source issue resolution.
