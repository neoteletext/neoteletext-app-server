#!/usr/bin/env bash
# The project's one check script (requirement 2.1): its own steps, on the
# gate machinery every neoteletext repository shares (scripts/gate.sh,
# copied from neoteletext-common; DEVELOPMENT.md section 9 there). Every step either
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
# The native server's systems besides the one the gate runs on
# (requirement 1.1, REQ-1020): each is type-checked by the full gate, so a
# `cfg` that stops compiling on a system is found without that system.
# Each is in rust-toolchain.toml's targets. Built and tested on each by
# hand, as REQ-1020's State records (TODO-2055).
readonly NATIVE_TARGETS=(x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu x86_64-apple-darwin aarch64-apple-darwin x86_64-pc-windows-msvc aarch64-pc-windows-msvc)
# What the gates read.
readonly CHECKED_PATHS=(crates fuzz scripts tests .githooks Cargo.toml Cargo.lock rust-toolchain.toml clippy.toml rustfmt.toml deny.toml)

# The TypeScript kit's repository, and the commit whose
# test/fixtures/hello.jsonl the server's tests/fixtures/kit-channel.jsonl is
# a copy of (TODO-1537): the kit's test checks its fixture channel writes
# those lines, the server's test serves them, and this step keeps the two
# the same, so a change to either fails until both are taken.
kit="${NEOTELETEXT_TS_PROTOCOL:-$HOME/devel/neoteletext-server-ts-protocol}"
readonly KIT_FIXTURE_FROM="a129389e3c055d8f93635c96f465e694cc4ca948"
readonly KIT_FIXTURE_COPY="tests/fixtures/kit-channel.jsonl"
kit_git() { env -u GIT_DIR -u GIT_INDEX_FILE -u GIT_WORK_TREE git -C "$kit" "$@"; }

check_kit_fixture() {
    step "$KIT_FIXTURE_COPY is the TypeScript kit's test/fixtures/hello.jsonl at ${KIT_FIXTURE_FROM:0:7}"
    kit_git rev-parse --git-dir >/dev/null 2>&1 || fail "the TypeScript kit's repository is needed at $kit (or NEOTELETEXT_TS_PROTOCOL) to check the fixture's copy"
    kit_git cat-file -e "$KIT_FIXTURE_FROM:test/fixtures/hello.jsonl" 2>/dev/null || fail "$kit has no test/fixtures/hello.jsonl at $KIT_FIXTURE_FROM"
    cmp -s "$KIT_FIXTURE_COPY" <(kit_git show "$KIT_FIXTURE_FROM:test/fixtures/hello.jsonl") ||
        fail "$KIT_FIXTURE_COPY differs from the kit's test/fixtures/hello.jsonl at $KIT_FIXTURE_FROM; copy that file, or name in scripts/check.sh the commit the copy is from"
}

# neoteletext-common's repository, and the commit whose wire-vectors.tsv
# holds the rules and records rows the server's protocol tests write
# (RULE-85): crates/teletext-runtime/src/protocol/tests/rules-vectors.tsv
# is held equal to those rows of it by the tests, which write the file,
# and by this step, so a change to either fails until both agree.
common="${NEOTELETEXT_COMMON:-$HOME/devel/neoteletext-common}"
readonly RULES_VECTORS_FROM="ba39b431c6dbc2db64f67330cb07cc503771e3a4"
readonly RULES_VECTORS_COPY="crates/teletext-runtime/src/protocol/tests/rules-vectors.tsv"
common_git() { env -u GIT_DIR -u GIT_INDEX_FILE -u GIT_WORK_TREE git -C "$common" "$@"; }

check_rules_vectors() {
    step "$RULES_VECTORS_COPY is neoteletext-common's wire-vectors.tsv rows at ${RULES_VECTORS_FROM:0:7}"
    common_git rev-parse --git-dir >/dev/null 2>&1 || fail "neoteletext-common's repository is needed at $common (or NEOTELETEXT_COMMON) to check the rules vectors"
    common_git cat-file -e "$RULES_VECTORS_FROM:wire-vectors.tsv" 2>/dev/null || fail "$common has no wire-vectors.tsv at $RULES_VECTORS_FROM"
    local ours theirs
    ours="$(grep -v '^#' "$RULES_VECTORS_COPY")"
    [[ -n "$ours" ]] || fail "$RULES_VECTORS_COPY has no rows"
    # The shared file's rows of the same kinds and names, in its order.
    theirs="$(common_git show "$RULES_VECTORS_FROM:wire-vectors.tsv" |
        awk -F '\t' 'NR == FNR { wanted[$1 FS $2] = 1; next } ($1 FS $2) in wanted' <(printf '%s\n' "$ours") -)"
    [[ "$ours" == "$theirs" ]] ||
        fail "$RULES_VECTORS_COPY differs from neoteletext-common's wire-vectors.tsv at ${RULES_VECTORS_FROM:0:7}: carry the rows across, or name in scripts/check.sh the commit they agree at"
}

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
    local manifests=(Cargo.toml crates/*/Cargo.toml fuzz/Cargo.toml) revs
    revs="$(sed -n 's/^neoteletext-[a-z0-9-]* = { git = "[^"]*", rev = "\([0-9a-f]*\)".*/\1/p' "${manifests[@]}" | sort -u)"
    [[ -n "$revs" && "$revs" != *$'\n'* ]] || fail "every engine crate must name one commit, found: ${revs:-none}"
}

# DEC-1759 lesson 19: a receiver is known by the capabilities in HELLO alone.
# Its `User-Agent` and HELLO's agent text are never read, so nothing can
# branch on a product's name.
check_no_agent_text() {
    step "no server code reads a receiver's agent text"
    if git grep -nEi 'user[-_]agent|hello \{[^}]*agent' -- 'crates/*.rs' ':!crates/*/tests/*' ':!crates/*/tests.rs' ':!crates/*/bin/*'; then
        fail "server code reads a receiver's agent text (DEC-1759 lesson 19)"
    fi
}

# REQ-6608: every limit of a page's rule table is a constant of the
# engine's neoteletext_rules::limits, used by name; no file here states its
# figure beside its name. The limits are read from the pinned engine's own
# limits.rs; the check is tested first on a repository made for it.
check_limit_figures() {
    step "no figure of REQ-6608 beside a limit's name"
    need_tool jq "install jq (the system package), which reads cargo metadata here"
    scripts/test-limit-figures.sh >/dev/null
    local manifest
    manifest="$(cargo metadata --quiet --locked --format-version 1 | jq -r '.packages[] | select(.name == "neoteletext-rules") | .manifest_path')"
    [[ -n "$manifest" && "$manifest" != null ]] || fail "the engine's neoteletext-rules is not among the workspace's packages"
    scripts/limit-figures.sh "${manifest%Cargo.toml}src/limits.rs" . || fail "a limit's figure is written beside its name: use the constant (REQ-6608)"
}

# REQ-6967: every bound on HTTP/2, HTTP/3 and QUIC input is a named
# constant, defined once in the native host's limits module; the HTTP/3
# layer's own are the engine's (neoteletext_http3::limits, REQ-6986), held
# to one definition by the engine's gate. REQ-6974: so are the two bounds on
# the PROXY protocol header, the last two of the native host's.
readonly DIRECT_BOUNDS=(
    "crates/teletext-native/src/limits.rs:HANDSHAKE_WITHIN MAX_CONCURRENT_STREAMS MAX_UNIDIRECTIONAL_STREAMS MAX_HEADER_LIST_BYTES H2_MAX_FRAME_BYTES H2_PING_AFTER H2_DEAD_AFTER_PING MAX_QUIC_CONNECTIONS QUIC_STREAM_WINDOW_BYTES QUIC_CONNECTION_WINDOW_BYTES QUIC_IDLE_AFTER H3_HEAD_WITHIN H3_READ_BYTES H3_DATA_FRAME_BYTES MAX_PROXY_HEADER_BYTES PROXY_HEADER_WITHIN QUIC_CLOSED_WITHIN"
)

check_direct_bounds() {
    step "each bound of REQ-6967 and REQ-6974 is a constant defined once"
    local entry file name found
    for entry in "${DIRECT_BOUNDS[@]}"; do
        file="${entry%%:*}"
        for name in ${entry#*:}; do
            found="$(git grep -lE "const $name:" -- 'crates/*.rs' | tr '\n' ' ')"
            [[ "$found" == "$file " ]] || fail "$name must be defined in $file and nowhere else; found in: ${found:-no file}"
            [[ "$(git grep -cE "const $name:" -- "$file" | cut -d: -f2)" == 1 ]] || fail "$name is defined more than once in $file"
        done
    done
}

check_workspace() {
    check_engine_pin
    check_direct_bounds
    check_limit_figures
    check_rules_vectors
    check_kit_fixture
    check_no_agent_text
    step "lockfiles up to date"
    cargo metadata --quiet --locked --format-version 1 >/dev/null
    (cd fuzz && cargo metadata --quiet --locked --format-version 1 >/dev/null)
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
    cargo nextest run --workspace --hide-progress-bar --status-level fail --final-status-level fail
}

commit_steps() { check_workspace; }
push_steps() { check_shapes_and_tests; }

check_fuzz() {
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
    local target
    for target in "${NATIVE_TARGETS[@]}"; do
        step "the native server type-checks for $target"
        cargo check --quiet --locked -p teletext-native --all-targets --target "$target"
    done
    step "documentation tests"
    cargo test --quiet --workspace --doc
    step "documentation builds, warnings as errors"
    RUSTDOCFLAGS="-D warnings" cargo doc --quiet --workspace --no-deps
    step "release tests"
    cargo nextest run --workspace --release --hide-progress-bar --status-level fail --final-status-level fail
    step "tests with every optional feature compiled out"
    cargo nextest run --workspace --no-default-features --hide-progress-bar --status-level fail --final-status-level fail
    step "licences, advisories and sources"
    cargo deny check
    step "no unused dependencies"
    cargo machete
    check_fuzz
}

linux_steps() {
    step "Linux on $linux_host: Clippy and tests"
    need_host
    # By content, with the host's own times (DEVELOPMENT.md section 9.3).
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
