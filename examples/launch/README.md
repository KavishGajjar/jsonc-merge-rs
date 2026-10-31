# Launch configuration example

Ours moves the worker before the API. Theirs changes the API program to its built
location. The merge keeps both edits and keeps each record's comment with it.
This is a constructed example of an editor configuration workflow, not a user study.

From the repository root:

```sh
cargo run -p jsonc-merge-cli -- merge \
  examples/launch/base.jsonc examples/launch/ours.jsonc examples/launch/theirs.jsonc \
  --policy examples/launch/policy.json
```

`expected.json` specifies the result's JSON value. `expected.jsonc` is the actual
source-preserving output, including comments. The README demo and packaged binary
checks both use this golden file.
