#!/usr/bin/env bash
# The project's one check script (requirement 2.1). Every step either
# passes or fails the run: nothing is skipped, a missing tool or an
# unreachable host is a failure.
#   scripts/check.sh commit   the pre-commit gate: formatting, lints in
#                             every shape and tests, refused while the
#                             checked parts have changes that are not
#                             staged, so what is checked is what is
#                             committed
#   scripts/check.sh full     the pre-push gate: everything, on a clean
#                             tree at the commit being pushed, including
#                             release tests, licences and advisories,
#                             unused dependencies, fuzzing, and the Linux
#                             build and tests on $TELETEXT_LINUX_HOST
# Both gates run the Python stage (check_python) and the TypeScript stage
# (scripts/typescript.sh) for the channels' programs and their kits.
set -euo pipefail
cd "$(dirname "$0")/.."

mode="${1:-}"
# The Linux host of the neoteletext full gates (DEVELOPMENT.md section 12).
linux_host="${TELETEXT_LINUX_HOST:-foundintelquiet4060}"
# Seconds each fuzz target runs in the full gate, after replaying every
# input that has ever found a fault (fuzz/regressions/).
fuzz_seconds="${TELETEXT_FUZZ_SECONDS:-20}"
# Cloudflare Workers' target, which the runtime must build for
# (requirement 1.3).
readonly WASM_TARGET="wasm32-unknown-unknown"
# The crates that build for WebAssembly: everything a Worker links.
readonly WASM_CRATES=(teletext-runtime)

step() { printf '\n== %s\n' "$*"; }
fail() {
    printf '\nFAILED: %s\n' "$*" >&2
    exit 1
}

. scripts/tools.sh
out_of_date="$(tools_out_of_date)"
[[ -z "$out_of_date" ]] || fail "tools missing or at another version (run scripts/tools.sh):
$out_of_date"

# The crates that read input from outside their process, each with the
# same no-panic lint list at its root. Every crate root must be listed.
readonly NO_PANIC_ROOTS=(crates/teletext-runtime/src/lib.rs crates/teletext-native/src/lib.rs crates/teletext-native/src/main.rs
    crates/teletext-native/src/bin/teletext-load.rs)

check_no_panic_lists() {
    step "the no-panic lint list is the same in every crate root"
    local first="" root list
    for root in crates/*/src/lib.rs crates/*/src/main.rs crates/*/src/bin/*.rs; do
        [[ -f "$root" ]] || continue
        [[ " ${NO_PANIC_ROOTS[*]} " == *" $root "* ]] || fail "$root is a crate root missing from NO_PANIC_ROOTS"
    done
    for root in "${NO_PANIC_ROOTS[@]}"; do
        [[ -f "$root" ]] || fail "$root is listed in NO_PANIC_ROOTS but does not exist"
        list="$(sed -n '/^#!\[deny(/,/^)\]/p' "$root")"
        [[ -n "$list" ]] || fail "$root has no #![deny(...)] list"
        if [[ -z "$first" ]]; then
            first="$list"
        elif [[ "$list" != "$first" ]]; then
            fail "$root's #![deny(...)] list differs from ${NO_PANIC_ROOTS[0]}'s"
        fi
    done
}

# The engine is imported at one commit, never copied (CLAUDE.md).
check_engine_pin() {
    step "the engine is pinned at one commit"
    local manifests=(Cargo.toml crates/*/Cargo.toml) revs
    [[ -f fuzz/Cargo.toml ]] && manifests+=(fuzz/Cargo.toml)
    revs="$(sed -n 's/^neoteletext-[a-z0-9-]* = { git = "file:[^"]*", rev = "\([0-9a-f]*\)".*/\1/p' "${manifests[@]}" | sort -u)"
    if grep -q '^neoteletext-[a-z0-9-]* = ' "${manifests[@]}"; then
        [[ -n "$revs" && "$revs" != *$'\n'* ]] || fail "every engine crate must name one commit, found: ${revs:-none}"
    else
        printf 'no engine crate imported yet\n'
    fi
}

check_workspace() {
    check_engine_pin
    step "lockfiles up to date"
    cargo metadata --quiet --locked --format-version 1 >/dev/null
    if [[ -f fuzz/Cargo.toml ]]; then
        (cd fuzz && cargo metadata --quiet --locked --format-version 1 >/dev/null)
    fi
    step "formatting"
    cargo fmt --check
    check_no_panic_lists
    step "Clippy, warnings as errors"
    cargo clippy --quiet --workspace --all-targets -- -D warnings
    step "Clippy with every optional feature compiled out, warnings as errors"
    cargo clippy --quiet --workspace --all-targets --no-default-features -- -D warnings
    step "Clippy for $WASM_TARGET, warnings as errors: ${WASM_CRATES[*]}"
    local crate packages=()
    for crate in "${WASM_CRATES[@]}"; do packages+=(-p "$crate"); done
    cargo clippy --quiet "${packages[@]}" --target "$WASM_TARGET" -- -D warnings
    step "tests"
    cargo nextest run --workspace --hide-progress-bar --status-level fail --final-status-level fail --no-tests=warn
}

# The Python side (docs/applications.md section 5): the kit and the
# channels written in Python, their tools at the versions uv.lock pins.
PYTHON_PATHS=(kits/python channels/hackaday pyproject.toml uv.lock)
check_python() {
    step "Python: format, lint, types and tests, with uv.lock's tools"
    command -v uv >/dev/null || fail "uv is needed for the Python checks"
    local folders=() folder
    for folder in kits/python channels/*/; do
        [[ -f "$folder/pyproject.toml" ]] && folders+=("${folder%/}")
    done
    uv sync --frozen --quiet
    uv run --frozen ruff format --check --quiet "${folders[@]}"
    uv run --frozen ruff check --quiet "${folders[@]}"
    uv run --frozen mypy "${folders[@]}"
    uv run --frozen pytest --quiet "${folders[@]}"
}

# Paths with changes that are not staged, or files git does not track,
# under any of the given paths.
unstaged_under() {
    git diff --name-only -- "$@"
    git ls-files --others --exclude-standard -- "$@"
}

# The paths of the TypeScript stage: the kit, the channels' programs, the
# channels file and Node's tools.
readonly TYPESCRIPT_PATHS=(kits channels channels.json package.json package-lock.json tsconfig.json scripts .githooks)
# The Rust test that runs the server with a TypeScript program and reads
# back the pages it publishes, so a change to the kit is checked against
# the server's TTI reader.
readonly TYPESCRIPT_SERVER_TEST="typescript_channel"

check_typescript() {
    scripts/typescript.sh
    step "the server serves a TypeScript program's pages"
    cargo nextest run --package teletext-native --test "$TYPESCRIPT_SERVER_TEST" --hide-progress-bar --status-level fail --final-status-level fail
}

commit_gate() {
    local staged
    staged="$(git diff --cached --name-only)"
    if [[ -z "$staged" ]]; then
        step "nothing staged"
        return
    fi
    local code=no python=no typescript=no path
    while IFS= read -r path; do
        case "$path" in
        crates/* | fuzz/* | scripts/* | .githooks/* | Cargo.toml | Cargo.lock | rust-toolchain.toml | clippy.toml | rustfmt.toml | deny.toml) code=yes ;;
        kits/python/* | channels/hackaday/* | pyproject.toml | uv.lock) python=yes ;;
        esac
        case "$path" in
        kits/* | channels/* | channels.json | package.json | package-lock.json | tsconfig.json | scripts/* | .githooks/*) typescript=yes ;;
        esac
    done <<<"$staged"
    # What is checked must be what is committed: no unstaged edit or
    # untracked file may sit in a part being checked.
    if [[ "$code" == yes ]]; then
        local loose
        loose="$(unstaged_under crates fuzz scripts .githooks Cargo.toml Cargo.lock rust-toolchain.toml clippy.toml rustfmt.toml deny.toml)"
        [[ -z "$loose" ]] || fail "changes that are not staged would make this check test something other than the commit; stage or stash them:
$loose"
        check_workspace
    fi
    if [[ "$python" == yes ]]; then
        local loose
        loose="$(unstaged_under "${PYTHON_PATHS[@]}")"
        [[ -z "$loose" ]] || fail "changes that are not staged would make this check test something other than the commit; stage or stash them:
$loose"
        check_python
    fi
    if [[ "$typescript" == yes ]]; then
        local loose
        loose="$(unstaged_under "${TYPESCRIPT_PATHS[@]}")"
        [[ -z "$loose" ]] || fail "changes that are not staged would make this check test something other than the commit; stage or stash them:
$loose"
        check_typescript
    fi
    return 0
}

check_linux() {
    step "Linux on $linux_host: Clippy and tests, Rust and TypeScript"
    ssh -o BatchMode=yes -o ConnectTimeout=10 "$linux_host" true || fail "$linux_host is unreachable; the full gate needs it"
    # A Python environment and its caches are this machine's, built for its
    # system; uv makes the host's own.
    rsync -a --delete --exclude target --exclude tools --exclude .git --exclude fuzz/corpus --exclude fuzz/artifacts --exclude node_modules \
        --exclude .venv --exclude __pycache__ --exclude .mypy_cache --exclude .ruff_cache --exclude .pytest_cache \
        ./ "$linux_host:teletext-server-check/"
    # The engine is a git dependency on a repository on this machine; its
    # history goes to the host, and git there reads it in place of this
    # machine's path, for these commands only.
    local engine
    engine="$(sed -n 's/^neoteletext-[a-z0-9-]* = { git = "\(file:[^"]*\)".*/\1/p' Cargo.toml crates/*/Cargo.toml | sort -u)"
    [[ "$engine" != *$'\n'* ]] || fail "the manifests must name one engine repository, found: $engine"
    local config=""
    if [[ -n "$engine" ]]; then
        rsync -a --delete "${engine#file://}/.git/" "$linux_host:teletext-server-check-engine.git/"
        config="export CARGO_NET_GIT_FETCH_WITH_CLI=true GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=url.file://\$HOME/teletext-server-check-engine.git.insteadOf GIT_CONFIG_VALUE_0=$engine;"
    fi
    # rustup installs the toolchain rust-toolchain.toml pins if the host
    # does not have it yet. uv is in ~/.local/bin, which a command over ssh
    # does not have on its path; the end-to-end test starts channels.json's
    # Hackaday with it, as the deployed service does (scripts/deploy.sh).
    # shellcheck disable=SC2029 # the engine is expanded here on purpose
    ssh "$linux_host" "set -e; . ~/.cargo/env; export PATH=\$HOME/.local/bin:\$PATH; cd teletext-server-check; rustup toolchain install; $config
        cargo clippy --quiet --workspace --all-targets -- -D warnings && cargo test --quiet --workspace \\
            && cargo clippy --quiet --workspace --all-targets --no-default-features -- -D warnings && scripts/typescript.sh"
}

check_fuzz() {
    [[ -f fuzz/Cargo.toml ]] || {
        step "fuzzing: no fuzz workspace yet"
        return 0
    }
    step "fuzz: Clippy, then $fuzz_seconds seconds a target after its regressions"
    (
        cd fuzz
        cargo "+$FUZZ_NIGHTLY" clippy --quiet --all-targets -- -D warnings
        local target log
        for target in $(cargo "+$FUZZ_NIGHTLY" fuzz list); do
            mkdir -p "corpus/$target" "regressions/$target" target
            log="target/fuzz-$target.log"
            cargo "+$FUZZ_NIGHTLY" fuzz run "$target" "corpus/$target" "regressions/$target" -- \
                -max_total_time="$fuzz_seconds" -timeout=10 -rss_limit_mb=2048 >"$log" 2>&1 ||
                { tail -40 "$log"; fail "fuzz target $target found a fault; its input is in fuzz/artifacts/$target/"; }
            printf '%s: %s\n' "$target" "$(grep '^Done' "$log")"
        done
    )
}

full_gate() {
    # The commit being pushed is the one checked: the pre-push hook passes
    # the pushed commits, and the tree must be clean at HEAD.
    local pushed
    for pushed in "$@"; do
        [[ "$pushed" == "$(git rev-parse HEAD)" ]] || fail "pushing $pushed, but HEAD is $(git rev-parse HEAD); check out what you push"
    done
    local loose
    loose="$(unstaged_under .; git diff --cached --name-only)"
    [[ -z "$loose" ]] || fail "the tree has changes that are not committed; the full gate checks the commit being pushed:
$loose"

    check_workspace
    check_python
    check_typescript
    step "documentation tests"
    cargo test --quiet --workspace --doc
    step "documentation builds, warnings as errors"
    RUSTDOCFLAGS="-D warnings" cargo doc --quiet --workspace --no-deps
    step "release tests"
    cargo nextest run --workspace --release --hide-progress-bar --status-level fail --final-status-level fail --no-tests=warn
    step "tests with every optional feature compiled out"
    cargo nextest run --workspace --no-default-features --hide-progress-bar --status-level fail --final-status-level fail --no-tests=warn
    step "licences, advisories and sources"
    cargo deny check
    step "no unused dependencies"
    cargo machete
    check_fuzz
    check_linux

    step "the gate changed nothing that is committed"
    loose="$(git status --porcelain --untracked-files=no)"
    [[ -z "$loose" ]] || fail "running the gate changed committed files, so what passed is not what is committed:
$loose"
}

case "$mode" in
commit) commit_gate ;;
full)
    shift
    full_gate "$@"
    ;;
*) fail "usage: scripts/check.sh commit | full [pushed commit]..." ;;
esac

step "All checks passed ($mode)"
