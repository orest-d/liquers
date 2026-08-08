// STUBS06 — usage that the type checker must REJECT.
//
// Each block below is a mistake a real caller would plausibly make, and each must be a `tsc` error.
// `check-stubs.sh` asserts that this file fails to compile, and that the failure count matches the
// number of mistakes here — so a declaration change that makes any one of these legal is caught,
// not merely a change that makes them all legal.
//
// **Expected errors: 6.** Keep that number in step with `check-stubs.sh`.

import * as liquers from '../../examples-web/quickstart/dist/liquers_web.js';

// 1. `name` is required.
liquers.registerCommand({
  run: () => 1,
});

// 2. `run` is required.
liquers.registerCommand({
  name: 'no_run',
});

// 3. A misspelled field. Without a declared interface this would type-check as `any` and fail
//    silently at runtime — which is why the declarations carry `LiquersCommandSpec`.
liquers.registerCommand({
  name: 'typo',
  run: () => 1,
  argumnets: [{ name: 'n' }],
});

// 4. An argument type that does not exist.
liquers.registerCommand({
  name: 'bad_type',
  run: (n: number) => n,
  arguments: [{ name: 'n', type: 'int64' }],
});

// 5. A state mode that does not exist.
liquers.registerCommand({
  name: 'bad_state',
  state: 'raw',
  run: (v: unknown) => v,
});

// 6. A default that is not JSON-representable. The planner resolves defaults without re-entering
//    JavaScript, so a function default could never be honoured.
liquers.registerCommand({
  name: 'bad_default',
  run: (n: number) => n,
  arguments: [{ name: 'n', type: 'int', default: () => 2 }],
});
