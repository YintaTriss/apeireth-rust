#!/bin/bash
# Test 守门 1 跟 release-prep 守门 #1 一样的逻辑
BASE="13b91f81"
HEAD="dfc3bf68"
LOCKED="apeireth-supervisor apeireth-agent apeireth-council apeireth-bus apeireth-protocol apeireth-mcp apeireth-tool-registry apeireth-tool-runtime apeireth-graph apeireth-pipeline apeireth-tool-approval apeireth-extension apeireth-evolution apeireth-api apeireth-core apeireth-memory apeireth-asi apeireth-tools apeireth-cli apeireth-bench apeireth-cognition apeireth-action apeireth-life-force apeireth-constraint"
LOCKED_REGEX=$(echo "$LOCKED" | tr ' ' '|')
HITS=$(git diff --name-only "$BASE...$HEAD" -- 'crates/*.rs' 'crates/*/Cargo.toml' 2>/dev/null | grep -E "^crates/($LOCKED_REGEX)/" || true)
echo "Hits: $HITS"
echo ""

while IFS= read -r f; do
    [ -z "$f" ] && continue
    case "$f" in
        *.rs) ;;
        *) echo "  logic 改 (非 .rs): $f"; continue;;
    esac
    WORD_DIFF=$(git diff --word-diff=porcelain "$BASE...$HEAD" -- "$f" 2>/dev/null | grep -E '^[+-][^+-]' || true)
    if [ -z "$WORD_DIFF" ]; then
        echo "  pure fmt: $f"
    else
        echo "  logic 改: $f (有 word-level 增删):"
        echo "$WORD_DIFF" | head -5
    fi
done <<< "$HITS"