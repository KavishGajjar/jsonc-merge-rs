# Optional repository-local Git driver

Build/install the local CLI first. Then add a policy file (for example
`.jsonc-merge-policy.json`) to your project and configure the driver in that
project's local Git configuration:

```sh
git config --local merge.jsonc.name 'Identity-aware JSONC merge'
git config --local merge.jsonc.driver 'jsonc-merge git-driver %O %A %B %L --policy .jsonc-merge-policy.json'
```

Add narrowly scoped attributes appropriate to your repository:

```gitattributes
.vscode/launch.json merge=jsonc
.vscode/tasks.json merge=jsonc
```

Git executes drivers from the working tree root. The executable must be on Git's
PATH; quote an absolute executable path inside the driver string if needed.
Remove `--policy ...` for conservative ordinary-object/atomic-array merging.
Do not install the driver globally unless you independently want that behavior.
This project never changes your Git configuration automatically.

The arguments are Git's original, current and other temporary files followed by
the requested marker width (`%O`, `%A`, `%B`, `%L`). Width defaults to 7 and supports
1–80. All three scratch inputs must be regular files; symbolic links are refused.
Normal merge/plan/check commands may read symlinks but never modify their inputs.

On a clean merge the driver prepares and revalidates the complete JSONC result,
writes a sibling temporary file, syncs it and replaces Git's `%A` file, preserving
its file permissions. It exits 0. It does not stage or commit anything.

On a semantic conflict, it writes a whole-file diff3 conflict to `%A` and exits
1. The sections are OURS, BASE and THEIRS, with Git's requested marker width.
The stderr JSON report describes semantic conflicts. Granular inline semantic
markers are outside v1; the marked file is deliberately not valid JSONC until
you resolve it. A text merge cannot override a semantic conflict.

Input/policy/operational failures exit 2; the driver does not overwrite `%A`
before it has a complete result. Git will report the merge as failed. Sibling
replacement is a single-file operation, not multi-file transaction support or a
power-loss durability guarantee. Inputs are expected to be Git-controlled scratch
files, not adversarial concurrently changing filesystem objects.

To remove the local registration:

```sh
git config --local --remove-section merge.jsonc
```

Also remove the corresponding attribute entries if you no longer want Git to
invoke that driver. Integration tests demonstrate clean and conflicted merges
in disposable repositories without touching global configuration.
