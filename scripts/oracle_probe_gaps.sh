#!/usr/bin/env bash
# Two open semantic questions, put to Apple's swcutil.
#
#   sudo ./scripts/oracle_probe_gaps.sh
#
# Both are places where this crate had to choose a reading because Apple's reference does not say.
# Neither affects any case in the current corpus; both would if Apple disagrees with us.
#
#   Q1  Unknown keys inside a component rule.
#       We ignore them. If Apple instead treats an unknown key as a constraint it cannot satisfy,
#       every rule carrying one is a FALSE POSITIVE for us: Apple says no, we say yes.
#
#   Q2  Pattern keys inside `defaults`.
#       Apple calls defaults "a subclass of components" but lists only caseSensitive and
#       percentEncoded. We apply those two and ignore the rest, reporting AASA192. If Apple really
#       inherits component properties there, we are missing a whole matching layer.
#
# Output: conformance/oracle/swcutil-gap-probes.tsv, same shape as swcutil-probes.tsv
#   group <TAB> name <TAB> subject <TAB> url <TAB> exit <TAB> single-line output
set -uo pipefail
cd "$(dirname "$0")/.."

if [ "$(uname -s)" != "Darwin" ]; then echo "swcutil exists only on macOS; skipping" >&2; exit 0; fi
if [ "$(id -u)" -ne 0 ]; then echo "swcutil must run as root; re-run with sudo" >&2; exit 1; fi

DOMAIN="${DOMAIN:-example.com}"
OUT="conformance/oracle/swcutil-gap-probes.tsv"
work="$(mktemp -d)"; trap 'rm -rf "$work"' EXIT
: > "$OUT"

flatten() { tr '\n' '|' | sed 's/  */ /g'; }

# swcutil match -u <url> -j <dict>: one pattern dictionary, one URL, no document around it.
probe_match() { # group name dict url
  local out; out="$(swcutil match -u "$4" -j "$3" 2>&1)"; local rc=$?
  printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$1" "$2" "$3" "$4" "$rc" "$(printf '%s' "$out" | flatten)" >> "$OUT"
  printf '  %-34s %s\n' "$2" "$(printf '%s' "$out" | head -1)"
}

# swcutil verify -d <domain> -j <file> -u <url>: a whole document, so `defaults` exists.
probe_verify() { # group name json url
  printf '%s' "$3" > "$work/apple-app-site-association"
  local out; out="$(swcutil verify -d "$DOMAIN" -j "$work/apple-app-site-association" -u "$4" 2>&1)"; local rc=$?
  printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$1" "$2" "$3" "$4" "$rc" "$(printf '%s' "$out" | flatten)" >> "$OUT"
  printf '  %-34s %s\n' "$2" "$(printf '%s' "$out" | grep -iE 'match|denied|approved|error' | head -1)"
}

echo "swcutil: $(swcutil --version 2>&1 | head -1)"
echo "domain:  $DOMAIN"
echo

echo "Q1  unknown keys inside a component rule"
echo "    we ignore them, so every line below should read the same as its control"
probe_match unknown-key control-plain          '{"/":"/foo/*"}'                        "https://$DOMAIN/foo/1"
probe_match unknown-key unknown-alongside-path '{"/":"/foo/*","totallyUnknownKey":"x"}' "https://$DOMAIN/foo/1"
probe_match unknown-key unknown-nonmatching    '{"/":"/foo/*","totallyUnknownKey":"x"}' "https://$DOMAIN/bar"
probe_match unknown-key unknown-alone          '{"totallyUnknownKey":"x"}'              "https://$DOMAIN/anything"
probe_match unknown-key control-empty          '{}'                                     "https://$DOMAIN/anything"
probe_match unknown-key misspelled-known       '{"/":"/foo/*","casesensitive":true}'    "https://$DOMAIN/FOO/1"
probe_match unknown-key nested-unknown-object  '{"/":"/foo/*","future":{"a":"b"}}'      "https://$DOMAIN/foo/1"
echo

echo "Q2  pattern keys inside defaults"
echo "    we ignore them, so each 'defaults-*' line should read the same as its control"
DOC_CONTROL='{"applinks":{"details":[{"appIDs":["ABCDE12345.com.example.app"],"components":[{"#":"*"}]}]}}'
probe_verify defaults control-no-defaults "$DOC_CONTROL" "https://$DOMAIN/bar"

# applinks-level defaults carrying a path pattern: if inherited, /bar must stop matching.
probe_verify defaults applinks-path \
  '{"applinks":{"defaults":{"/":"/foo/*"},"details":[{"appIDs":["ABCDE12345.com.example.app"],"components":[{"#":"*"}]}]}}' \
  "https://$DOMAIN/bar"
probe_verify defaults applinks-path-agreeing \
  '{"applinks":{"defaults":{"/":"/foo/*"},"details":[{"appIDs":["ABCDE12345.com.example.app"],"components":[{"#":"*"}]}]}}' \
  "https://$DOMAIN/foo/1"

# detail-level defaults, same question one level down.
probe_verify defaults detail-path \
  '{"applinks":{"details":[{"appIDs":["ABCDE12345.com.example.app"],"defaults":{"/":"/foo/*"},"components":[{"#":"*"}]}]}}' \
  "https://$DOMAIN/bar"

# exclude in defaults: if inherited, nothing can match at all.
probe_verify defaults applinks-exclude \
  '{"applinks":{"defaults":{"exclude":true},"details":[{"appIDs":["ABCDE12345.com.example.app"],"components":[{"#":"*"}]}]}}' \
  "https://$DOMAIN/bar"

# a query dictionary in defaults.
probe_verify defaults applinks-query \
  '{"applinks":{"defaults":{"?":{"id":"??"}},"details":[{"appIDs":["ABCDE12345.com.example.app"],"components":[{"#":"*"}]}]}}' \
  "https://$DOMAIN/bar?id=7"
probe_verify defaults applinks-query-violating \
  '{"applinks":{"defaults":{"?":{"id":"??"}},"details":[{"appIDs":["ABCDE12345.com.example.app"],"components":[{"#":"*"}]}]}}' \
  "https://$DOMAIN/bar?id=7777"

# control: the two keys Apple does document there, to prove defaults is read at all.
probe_verify defaults applinks-casesensitive-true \
  '{"applinks":{"defaults":{"caseSensitive":true},"details":[{"appIDs":["ABCDE12345.com.example.app"],"components":[{"/":"/foo"}]}]}}' \
  "https://$DOMAIN/FOO"
probe_verify defaults applinks-casesensitive-false \
  '{"applinks":{"defaults":{"caseSensitive":false},"details":[{"appIDs":["ABCDE12345.com.example.app"],"components":[{"/":"/foo"}]}]}}' \
  "https://$DOMAIN/FOO"
echo

cat <<'NOTE'
How to read this
----------------
Q1  If every "unknown-*" line agrees with its control, Apple ignores unknown component keys and our
    `_ => {}` is oracle-backed. Promote the cases into conformance/cases.json and say so in
    docs/parity.md.
    If "unknown-alongside-path" fails to match while "control-plain" matches, Apple treats an
    unknown key as an unsatisfiable constraint. That is a false-positive class for us and needs
    conservative handling: an unrecognised key in a component should make the rule not match.

Q2  The two "casesensitive" lines must disagree with each other. If they do not, `defaults` was not
    consulted and the rest of Q2 proves nothing — fix the probe before drawing a conclusion.
    Given that control holds: if the "applinks-path", "detail-path", "applinks-exclude" and
    "applinks-query*" lines all read the same as "control-no-defaults", Apple ignores pattern keys
    in defaults and AASA192 becomes oracle-backed rather than a judgement call.
    If any of them changes the outcome, `defaults` really is a components subclass, and the
    defaults hierarchy has to carry patterns too — a genuine missing layer.
NOTE
echo "raw: $OUT"
