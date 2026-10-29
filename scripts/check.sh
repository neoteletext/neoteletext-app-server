#!/usr/bin/env bash
# The project's one check script (requirement 2.1): its own steps, on the
# gate machinery every neoteletext repository shares (scripts/gate.sh,
# copied from neoteletext-common; gates.md there). Every step either
# passes or fails the run: nothing is skipped, a missing tool or an
# unreachable host is a failure.
#   scripts/check.sh commit   formatting and Clippy: the code compiles
#                             clean, on what is staged
#   scripts/check.sh push     Clippy in every shape and the tests: a push
#                             of livedevelopment
#   scripts/check.sh full     release tests, documentation, licences and
#                             advisories, unused dependencies, fuzzing, and
#                             the Linux build and tests on
#                             $TELETEXT_LINUX_HOST alongside: a push to main
# The channels' programs and their kits have their own gates, in
# ~/devel/neoteletext-server-channels (decision 12).
set -euo pipefail
cd "$(dirname "$0")/.."

# The Linux host of the neoteletext full gates (DEVELOPMENT.md section 12).
linux_host="${TELETEXT_LINUX_HOST:-foundintelquiet4060}"
# Seconds each fuzz target runs in the full gate, after replaying every
# input that has ever found a fault (fuzz/regressions/).
fuzz_seconds="${TELETEXT_FUZZ_SECONDS:-20}"
# Cloudflare Workers' target, which the runtime must build for
# (requirement 1.3).
readonly WASM_TARGET="wasm32-unknown-unknown"
# The crates that build for WebAssembly: everything a Worker links.
readonly WASM_CRATES=(teletext-runtime teletext-cloudflare)
# What the gates read.
readonly CHECKED_PATHS=(crates fuzz scripts .githooks Cargo.toml Cargo.lock rust-toolchain.toml clippy.toml rustfmt.toml deny.toml)

. scripts/gate.sh

. scripts/tools.sh
out_of_date="$(tools_out_of_date)"
[[ -z "$out_of_date" ]] || fail "tools missing or at another version (run scripts/tools.sh):
$out_of_date"

# The crates that read input from outside their process, each with the
# same no-panic lint list at its root. Every crate root must be listed.
readonly NO_PANIC_ROOTS=(crates/teletext-runtime/src/lib.rs crates/teletext-native/src/lib.rs crates/teletext-native/src/main.rs
    crates/teletext-native/src/bin/teletext-load.rs crates/teletext-cloudflare/src/lib.rs)

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
    revs="$(sed -n 's/^neoteletext-[a-z0-9-]* = { git = "[^"]*", rev = "\([0-9a-f]*\)".*/\1/p' "${manifests[@]}" | sort -u)"
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
}

# What the push gate adds: every shape, and the tests.
check_shapes_and_tests() {
    step "Clippy with every optional feature compiled out, warnings as errors"
    cargo clippy --quiet --workspace --all-targets --no-default-features -- -D warnings
    step "Clippy for $WASM_TARGET, warnings as errors: ${WASM_CRATES[*]}"
    local crate packages=()
    for crate in "${WASM_CRATES[@]}"; do packages+=(-p "$crate"); done
    cargo clippy --quiet "${packages[@]}" --target "$WASM_TARGET" -- -D warnings
    step "tests"
    cargo nextest run --workspace --hide-progress-bar --status-level fail --final-status-level fail --no-tests=warn
}

commit_steps() { check_workspace; }
push_steps() { check_shapes_and_tests; }

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

full_steps() {
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
}

linux_steps() {
    step "Linux on $linux_host: Clippy and tests"
    ssh -o BatchMode=yes -o ConnectTimeout=10 "$linux_host" true || fail "$linux_host is unreachable; the full gate needs it"
    # By content, with the host's own times (gates.md).
    rsync -a --no-times --checksum --delete --exclude target --exclude tools --exclude .git --exclude fuzz/corpus --exclude fuzz/artifacts \
        ./ "$linux_host:teletext-server-check/"
    # The engine is a git dependency on its public repository, which the
    # host fetches by itself. rustup installs the toolchain
    # rust-toolchain.toml pins if the host does not have it yet.
    ssh "$linux_host" "set -e; . ~/.cargo/env; cd teletext-server-check; rustup toolchain install
        cargo clippy --quiet --workspace --all-targets -- -D warnings && cargo test --quiet --workspace \\
            && cargo clippy --quiet --workspace --all-targets --no-default-features -- -D warnings"
}

gate_main "$@"
