#!/bin/bash
# Bevy plugins declared in TypeScript projects, resolved into raea's plugin build (sdk/raea/docs/bevy-plugins.md).
# A project names its plugins in package.json under raea.plugins (raea.bevy.plugins, its first name, is still read);
# this script turns every declaration into
# engine/bevy-plugins (its declared dependencies and features, and src/declared.rs, which registers each plugin with
# raea-render), builds raea-bevy-sdk with the plugins a project asks for, and generates that project's TypeScript from the
# built App's TypeRegistry, with a lock that fails loudly when a plugin's version moves without a regeneration.
#
#   tools/bevy-plugins.sh sync [PACKAGE.json ...]   write engine/bevy-plugins from the declarations (default: every
#                                                   package.json in the repo that declares plugins)
#   tools/bevy-plugins.sh build DIR                 sync, then build raea-bevy-sdk with DIR's plugins (through lane),
#                                                   then gen DIR when a plugin's version differs from DIR's lock
#   tools/bevy-plugins.sh gen DIR                   DIR/bevy/<plugin>.gen.tsx and DIR/bevy/plugins.lock.json (or in
#                                                   the folder package.json's raea.generated names, relative to DIR)
#   tools/bevy-plugins.sh check DIR                 fails when the declarations, Cargo.lock or the generated TypeScript
#                                                   have drifted from DIR's lock (CI runs this)
#   tools/bevy-plugins.sh features DIR              the cargo features DIR's plugins need
#
# One declaration, keyed by the plugin's name (that name is also its cargo feature):
#   "avian": {
#     "crate": "avian3d",                                     default: the key
#     "versions": { "0.19": "=0.7.0", "0.20": "=0.8.0" },     by Bevy minor: a crates.io requirement, or
#                                                             {"git": URL, "rev": REV} or {"path": DIR}; "version"
#                                                             alone is one requirement for every Bevy
#     "features": ["3d", "f32"],                              its default features are always off
#     "plugin": "avian3d::prelude::PhysicsPlugins::default()",  the Rust expression app.add_plugins takes
#     "calls": "avian",                                       optional: avian.rs's calls(app) adds methods (see CALLS)
#     "adapter": "hanabi",                                    the escape hatch: its register() does it all (see ADAPTERS)
#     "deps": { "rand_pcg": { "version": "0.10" } },          crates the adapter needs, behind the same feature
#     "types": ["avian3d::"],                                 type-path prefixes to generate (default: the crate's)
#     "reflect": { "components": ["bevy_egui::EguiContextSettings"],  types deriving Reflect that the crate leaves
#                  "resources": [...], "messages": [...],     unregistered or without #[reflect(Component)] (Resource,
#                  "defaults": [...] }                        Message, Default): registered here, so TypeScript can write them
#     "headless": ["assets", "mesh"],                         what a plain headless App needs for it
#     "onDemand": true,                                       only an App that names it gets it (a peer's half)
#     "library": true,                                        no plugin: compiled for its types and calls (noise)
#     "widgets": { "x-gauge": "gauge::Gauge::default()" },   Blitz custom widgets: each tag's elements in every
#                                                             page backed by the blitz-dom Widget its expression makes
#                                                             (a widget crate with no Bevy plugin is a "library")
#     "verdict": "blocked: ...",                              why its line for this Bevy is null, and when that was checked
#     "builtin": true                                         always compiled in, not a feature (bevy_remote); its
#                                                             features go behind a feature of its name. A builtin
#                                                             whose crate the manifest already has (bevy) is only that
#     "needs": ["avian"],                                     other declared plugins it needs (a backend's engine)
#     "raea": true                                            raea's own plugin, in every raea build (render's looks,
#                                                             audio's mixer): nothing for cargo; gen writes its handle,
#                                                             with the registry's types under "types" when it names them
#   }
# "versions" may say "0.20": null: no release for that Bevy yet, so a build on it leaves the plugin out (sync says
# so). "features" may be lists by Bevy minor when the feature names moved: {"0.19": [...], "0.20": [...]}.
# A project uses a plugin declared in another package by naming it there: "audio": { "from": "@raea/audio" } (the
# package.json whose "name" is @raea/audio declares it; its handle is that package's).
# "raea": { "generated": "../../../plugins/src" } beside "plugins" puts the generated modules in that folder instead of
# DIR/bevy, with an index.ts exporting each plugin's handle: the catalog bench's go to the SDK's @raea/plugins, which
# authors import as raea/plugins/<name> (docs/plugins.feature PL1).
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
CRATE=$ROOT/engine/bevy-plugins
TOML=$CRATE/Cargo.toml
DECLARED=$CRATE/src/declared.rs
# most calls live in their own crate (raea-bevy-calls): a declaration's "calls" module is there when its file is,
# else in this one (calls that need the bridge, avian's)
CALLS=$ROOT/engine/bevy-calls
CALLS_TOML=$CALLS/Cargo.toml
# and most adapters in theirs (raea-bevy-adapters), the same way: there when the file is, else in this crate (the ones
# that need its bridge, lightyear's)
ADAPTERS=$ROOT/engine/bevy-adapters
ADAPTERS_TOML=$ADAPTERS/Cargo.toml
SDK=$ROOT/target/release/raea-bevy-sdk

die() { echo "bevy-plugins: $*" >&2; exit 1; }
command -v jq >/dev/null || die "needs jq"

# the Bevy minor the tree builds against (the plugin crate's exact bevy pin)
bevy_minor() {
  sed -n 's/^bevy = { version = "=\([0-9]*\.[0-9]*\)\.[0-9]*".*/\1/p' "$TOML" | head -1
}

# a package.json's plugin declarations: raea.plugins (docs/sdk.feature S6b), over raea.bevy.plugins, its first name
PLUGINS='((.raea.bevy.plugins // {}) + (.raea.plugins // {}))'

# every package.json in the repo that declares plugins (tracked or new, not ignored)
declarations() {
  (cd "$ROOT" && git ls-files --cached --others --exclude-standard -- '*package.json' | while read -r f; do
    jq -e "$PLUGINS"' | length > 0' "$ROOT/$f" >/dev/null 2>&1 && echo "$ROOT/$f"
  done) || true
}

# the declarations merged into one object; the same name declared two ways is an error
merged() {
  local out='{}' f name a b
  for f in "$@"; do
    [ -f "$f" ] || die "no declaration $f"
    for name in $(jq -r "$PLUGINS"' | keys[]' "$f"); do
      # a reference to another package's declaration adds nothing of its own (spec_of checks it resolves)
      jq -e --arg n "$name" "$PLUGINS"'[$n] | type == "object" and has("from")' "$f" >/dev/null && { spec_of "$f" "$name" >/dev/null; continue; }
      a=$(jq -c --arg n "$name" '.[$n] // empty' <<<"$out")
      b=$(jq -c --arg n "$name" "$PLUGINS"'[$n] | del(.note)' "$f")
      if [ -n "$a" ] && [ "$a" != "$b" ]; then
        die "$name is declared two ways (one is in $f): $a vs $b. One Bevy links one version of a plugin, so declare it once."
      fi
      out=$(jq -c --arg n "$name" --argjson v "$b" '.[$n] = $v' <<<"$out")
    done
  done
  echo "$out"
}

# the package.json whose "name" is PACKAGE (@raea/audio), tracked or new
package_named() { # PACKAGE
  (cd "$ROOT" && git ls-files --cached --others --exclude-standard -- '*package.json' | while read -r f; do
    jq -e --arg p "$1" '.name == $p' "$ROOT/$f" >/dev/null 2>&1 && echo "$ROOT/$f"
  done) | head -1
}

# NAME's declaration in a package.json, a { "from": PACKAGE } reference followed to the package that declares it
spec_of() { # PACKAGE.json NAME
  local s from f
  s=$(jq -c --arg n "$2" "$PLUGINS"'[$n]' "$1")
  from=$(jq -r 'if type == "object" then .from // empty else empty end' <<<"$s")
  [ -n "$from" ] || { echo "$s"; return; }
  f=$(package_named "$from")
  [ -n "$f" ] || die "$2 in $1 is from $from, which no package.json in the repo is named"
  s=$(jq -c --arg n "$2" "$PLUGINS"'[$n] // empty' "$f")
  [ -n "$s" ] || die "$2 in $1 is from $from (${f#"$ROOT"/}), which doesn't declare $2"
  jq -e 'type == "object" and has("from")' <<<"$s" >/dev/null && die "$2 in ${f#"$ROOT"/} is itself a reference: declare it there"
  echo "$s"
}

# a crate's source for this Bevy as TOML keys: version = "..." | git = "...", rev = "..." | path = "..."
source_of() { # JSON-SPEC MINOR WHAT
  local s
  s=$(jq -c --arg m "$2" 'if .versions then .versions[$m] elif .version then .version else null end' <<<"$1")
  [ "$s" != null ] || die "$3 has no version for Bevy $2 (add \"$2\" to its versions)"
  jq -r 'if type == "string" then "version = \"\(.)\""
         elif .git then (if (.rev // "" | test("^[0-9a-f]{7,40}$")) then "git = \"\(.git)\", rev = \"\(.rev)\"" else error("\(.git): rev must be a commit hash, got \(.rev)") end)
         elif .path then "path = \"\(.path)\""
         else error("a source is a version, {git, rev} or {path}") end' <<<"$s" 2>/dev/null || die "$3's source for Bevy $2 isn't usable: $s"
}

toml_list() { jq -r '[.[] | "\"\(.)\""] | join(", ")'; }

# a plugin declared with no release for this Bevy ("0.20": null in its versions): left out of the build, said so
unreleased() { # JSON-SPEC MINOR
  jq -e --arg m "$2" '.versions and (.versions | has($m)) and .versions[$m] == null' <<<"$1" >/dev/null
}

# a plugin's features for this Bevy, as a JSON list: "features" is a list, or lists by Bevy minor
features_of() { # JSON-SPEC MINOR
  jq -c --arg m "$2" '(.features // []) | if type == "object" then (.[$m] // []) else . end' <<<"$1"
}

# the other declared plugins a plugin needs, as more entries of its cargo feature (tnua's avian backend needs avian)
needs_of() { # JSON-SPEC
  jq -r '[(.needs // [])[] | "\"\(.)\""] | if length > 0 then ", " + join(", ") else "" end' <<<"$1"
}

# the Rust that registers a plugin's "reflect" types with the App: a type that derives Reflect but whose crate never
# registers it (a library), or doesn't say #[reflect(Component)] / #[reflect(Resource)] / #[reflect(Message)], so
# TypeScript can't insert or write it; "defaults" are ones that implement Default without #[reflect(Default)], so a
# value that gives only some fields is refused. Each is a public Rust path; a generic one is named with its type arguments
reflect_of() { # JSON-SPEC
  jq -r '(.reflect // {}) as $r | "::bevy::ecs::reflect::" as $e
    | [($r.components // [])[] | " app.register_type::<\(.)>().register_type_data::<\(.), \($e)ReflectComponent>();"]
    + [($r.resources // [])[] | " app.register_type::<\(.)>().register_type_data::<\(.), \($e)ReflectResource>().register_type_data::<\(.), \($e)ReflectComponent>();"]
    + [($r.messages // [])[] | " app.register_type::<\(.)>().register_type_data::<\(.), \($e)ReflectMessage>();"]
    + [($r.defaults // [])[] | " app.register_type::<\(.)>().register_type_data::<\(.), ::bevy::prelude::ReflectDefault>();"]
    | join("")' <<<"$1"
}

# the plugins DIR builds: the ones it declares that have a release for this Bevy
released_names() { # PACKAGE.json
  local m name; m=$(bevy_minor)
  for name in $(jq -r "$PLUGINS"' | keys_unsorted[]' "$1"); do
    unreleased "$(spec_of "$1" "$name")" "$m" || echo "$name"
  done
}

# the plugins DIR builds that are cargo features: its released ones but raea's own (in every build already)
cargo_names() { # PACKAGE.json
  local name
  for name in $(released_names "$1"); do
    jq -e '.raea == true' <<<"$(spec_of "$1" "$name")" >/dev/null || echo "$name"
  done
}

# replace the lines between a ">>> NAME" and "<<< NAME" marker in a file
splice() { # FILE NAME CONTENT-FILE
  awk -v name="$2" -v body="$3" '
    index($0, ">>> " name) { print; while ((getline l < body) > 0) print l; skip = 1; next }
    index($0, "<<< " name) { skip = 0 }
    !skip { print }' "$1" > "$1.tmp" && mv "$1.tmp" "$1"
}

cmd_sync() {
  local files=("$@")
  [ $# -gt 0 ] || while read -r f; do files+=("$f"); done < <(declarations)
  [ ${#files[@]} -gt 0 ] || die "no package.json declares raea.plugins"
  local all minor deps feats cdeps cfeats reg list name spec crate src fl dname dspec dline seen="" skipped="" own=""
  all=$(merged "${files[@]}")
  minor=$(bevy_minor)
  [ -n "$minor" ] || die "can't read the bevy pin in $TOML"
  deps=$(mktemp); feats=$(mktemp); cdeps=$(mktemp); cfeats=$(mktemp); adeps=$(mktemp); afeats=$(mktemp); reg=$(mktemp); list=$(mktemp)
  for name in $(jq -r 'keys[]' <<<"$all"); do
    [[ "$name" =~ ^[a-z][a-z0-9_]*$ ]] || die "plugin name $name: lowercase letters, digits and _ (it is a cargo feature and a Rust name)"
    spec=$(jq -c --arg n "$name" '.[$n]' <<<"$all")
    crate=$(jq -r --arg n "$name" '.crate // $n' <<<"$spec")
    if unreleased "$spec" "$minor"; then
      skipped="$skipped $name"
      continue
    fi
    # raea's own plugin is in every raea build: nothing for cargo or the registry
    jq -e '.raea == true' <<<"$spec" >/dev/null && { own="$own $name"; continue; }
    src=$(source_of "$spec" "$minor" "$name")
    fl=$(features_of "$spec" "$minor" | toml_list)
    local builtin; builtin=$(jq -r '.builtin // false' <<<"$spec")
    local fdeps=""
    # calls in raea-bevy-calls, or an adapter in raea-bevy-adapters: the plugin's crate and its "deps" are that crate's
    # too, behind a feature of its name (its side)
    local calls in_calls=false cfdeps="" ad out_adapter=false side="" sdeps sfeats
    calls=$(jq -r '.calls // empty' <<<"$spec")
    [ -n "$calls" ] && [ -f "$CALLS/src/$calls.rs" ] && in_calls=true && side=calls && sdeps=$cdeps && sfeats=$cfeats
    ad=$(jq -r '.adapter // empty' <<<"$spec")
    if [ -n "$ad" ] && [ -f "$ADAPTERS/src/$ad.rs" ]; then
      [ -z "$side" ] || die "$name: calls in engine/bevy-calls and an adapter in engine/bevy-adapters; keep its code in one"
      out_adapter=true side=adapters sdeps=$adeps sfeats=$afeats
    fi
    # a library whose only code is those calls (no plugin, no types to register here), or a plugin an adapter there
    # installs, is that crate's alone
    local here=true
    [ $in_calls = true ] && [ "$(jq -r '(.library // false) and (.reflect | not)' <<<"$spec")" = true ] && here=false
    [ $out_adapter = true ] && here=false
    if [ "$builtin" = true ]; then
      # always compiled in, with no features; its features go behind a feature of its own name, so a workspace build
      # (which compiles this crate with none) doesn't unify them into everyone's Bevy. A crate the manifest already
      # depends on by hand (bevy itself) gets only the feature
      grep -q "^$crate = " <(sed '/>>> declared dependencies/,$d' "$TOML") || echo "$crate = { $src, default-features = false }" >> "$deps"
      fdeps=$(features_of "$spec" "$minor" | jq -r --arg c "$crate" '[.[] | "\"\($c)/\(.)\""] | join(", ")')
    else
      if [ $here = true ]; then
        echo "$crate = { $src, optional = true, default-features = false, features = [$fl] }" >> "$deps"
        fdeps="\"dep:$crate\""
      fi
      if [ -n "$side" ]; then
        echo "$crate = { $src, optional = true, default-features = false, features = [$fl] }" >> "$sdeps"
        cfdeps="\"dep:$crate\""
      fi
    fi
    for dname in $(jq -r '.deps // {} | keys[]' <<<"$spec"); do
      dspec=$(jq -c --arg d "$dname" '.deps[$d]' <<<"$spec")
      case " $seen " in *" $dname "*) die "$dname is a dependency of two plugins: declare it in one" ;; esac
      seen="$seen $dname"
      dline="$dname = { $(source_of "$dspec" "$minor" "$name's $dname"), optional = true, default-features = false, features = [$(jq '.features // []' <<<"$dspec" | toml_list)] }"
      if [ -n "$side" ]; then
        echo "$dline" >> "$sdeps"
        cfdeps="${cfdeps:+$cfdeps, }\"dep:$dname\""
      else
        echo "$dline" >> "$deps"
        fdeps="${fdeps:+$fdeps, }\"dep:$dname\""
      fi
    done
    [ -n "$side" ] && { echo "$name = [$cfdeps]" >> "$sfeats"; fdeps="${fdeps:+$fdeps, }\"raea-bevy-$side/$name\""; }
    echo "$name = [$fdeps$(needs_of "$spec")]" >> "$feats"
    # the registration raea-render sees, and what the generator and the bridge read
    local prefixes headless ondemand plugin adapter rcrate
    rcrate=${crate//-/_}
    prefixes=$(jq -c --arg c "$rcrate" '.types // ["\($c)::"]' <<<"$spec" | jq -r '[.[] | "\"\(.)\""] | join(", ")')
    headless=$(jq '.headless // []' <<<"$spec" | toml_list)
    ondemand=$(jq -r '.onDemand // false' <<<"$spec")
    plugin=$(jq -r '.plugin // empty' <<<"$spec")
    adapter=$(jq -r '.adapter // empty' <<<"$spec")
    local reflect; reflect=$(reflect_of "$spec")
    [ -z "$reflect" ] || [ -z "$adapter$(jq -r '.builtin // empty' <<<"$spec")" ] || die "$name: \"reflect\" is for a plugin or a library; an adapter or a builtin registers its own types"
    local cfg="#[cfg(feature = \"$name\")]"
    local lcfg="$cfg"
    [ "$builtin" = true ] && lcfg=""
    echo "    $lcfg v.push(Declared { name: \"$name\", krate: \"$crate\", source: $(jq -Rn --arg s "$src" '$s'), prefixes: &[$prefixes], needs: &[$(needs_of "$spec" | sed 's/^, //')], headless: &[$headless], on_demand: $ondemand });" >> "$list"
    # Blitz widgets (docs/sdk.feature S8b): each tag backed in every page App by the widget its expression makes
    local widgets; widgets=$(jq -r '.widgets // {} | to_entries[] | " raea_sys_page::widgets::AppPageWidgets::page_widget(app, \"\(.key)\", || Box::new(\(.value)));"' <<<"$spec" | tr -d '\n')
    [ -z "$widgets" ] || printf '    %s\n    raea_sys_page::widgets::register_page_plugin(|app| {%s });\n' "$cfg" "$widgets" >> "$reg"
    if [ "$builtin" = true ] && [ -z "$adapter" ]; then
      continue
    elif [ $out_adapter = true ]; then
      printf '    %s\n    raea_bevy_adapters::%s::register();\n' "$cfg" "$adapter" >> "$reg"
    elif [ -n "$adapter" ]; then
      [ -f "$CRATE/src/$adapter.rs" ] || die "$name's adapter src/$adapter.rs doesn't exist in engine/bevy-adapters or engine/bevy-plugins"
      printf '    %s\n    crate::%s::register();\n' "$cfg" "$adapter" >> "$reg"
    else
      local extra="$reflect" library
      library=$(jq -r '.library // false' <<<"$spec")
      if [ -n "$calls" ]; then
        if [ $in_calls = true ]; then extra="$extra raea_bevy_calls::$calls::calls(app);"
        else
          [ -f "$CRATE/src/$calls.rs" ] || die "$name's calls src/$calls.rs doesn't exist in engine/bevy-calls or engine/bevy-plugins"
          extra="$extra crate::$calls::calls(app);"
        fi
      fi
      if [ "$library" = true ]; then
        # a library has no plugin: it is compiled for its reflected types (registered here) and calls (its App half);
        # one with neither (bevy_asset_loader) is named, so it is linked and cargo-shear sees it used
        [ -z "$extra" ] || printf '    %s\n    raea_runtime_plugins::register(raea_runtime_plugins::Registered { name: "%s", install: |app| {%s }, group: None, on_demand: %s });\n' "$cfg" "$name" "$extra" "$ondemand" >> "$reg"
        [ -n "$extra" ] || printf '    %s\n    use %s as _;\n' "$cfg" "${crate//-/_}" >> "$reg"
        continue
      fi
      [ -n "$plugin" ] || die "$name needs \"plugin\" (the expression app.add_plugins takes), an \"adapter\", or \"library\": true"
      printf '    %s\n    raea_runtime_plugins::register(raea_runtime_plugins::Registered { name: "%s", install: |app| { crate::AddOnce::add_once(%s, app);%s }, group: None, on_demand: %s });\n' "$cfg" "$name" "$plugin" "$extra" "$ondemand" >> "$reg"
    fi
  done
  # the modules adapters and calls live in, by feature
  local mods cmods amods; mods=$(mktemp); cmods=$(mktemp); amods=$(mktemp)
  for name in $(jq -r 'keys[]' <<<"$all"); do
    spec=$(jq -c --arg n "$name" '.[$n]' <<<"$all")
    unreleased "$spec" "$minor" && continue
    for m in $(jq -r '[.adapter // empty, .calls // empty] | unique[]' <<<"$spec"); do
      if [ "$m" = "$(jq -r '.calls // empty' <<<"$spec")" ] && [ -f "$CALLS/src/$m.rs" ]; then
        printf '#[cfg(feature = "%s")]\npub mod %s;\n' "$name" "$m" >> "$cmods"
      elif [ "$m" = "$(jq -r '.adapter // empty' <<<"$spec")" ] && [ -f "$ADAPTERS/src/$m.rs" ]; then
        printf '#[cfg(feature = "%s")]\npub mod %s;\n' "$name" "$m" >> "$amods"
      else
        printf '#[cfg(feature = "%s")]\npub mod %s;\n' "$name" "$m" >> "$mods"
      fi
    done
  done
  # the catalog the release builds in (docs/nobuild.feature NB4a): every plugin but those declared "catalog": false
  echo "catalog = [$(jq -r 'to_entries[] | select(.value.catalog != false) | .key' <<<"$all" | grep -Fxf <(cut -d' ' -f1 "$feats") | sed 's/.*/"&"/' | paste -sd, - | sed 's/,/, /g')]" >> "$feats"
  splice "$TOML" "declared dependencies" "$deps"
  splice "$TOML" "declared features" "$feats"
  splice "$CRATE/src/lib.rs" "declared modules" "$mods"
  splice "$CALLS_TOML" "declared dependencies" "$cdeps"
  splice "$CALLS_TOML" "declared features" "$cfeats"
  splice "$CALLS/src/lib.rs" "declared modules" "$cmods"
  splice "$ADAPTERS_TOML" "declared dependencies" "$adeps"
  splice "$ADAPTERS_TOML" "declared features" "$afeats"
  splice "$ADAPTERS/src/lib.rs" "declared modules" "$amods"
  {
    echo "// Generated by tools/bevy-plugins.sh sync for Bevy $minor from:"
    # the files that declare what this crate builds (raea's own plugins and references to them add nothing here)
    for f in "${files[@]}"; do jq -e "$PLUGINS"' | any(.[]; type == "object" and (has("from") or .raea == true | not))' "$f" >/dev/null && echo "//   ${f#"$ROOT"/}" || true; done
    echo "// Don't edit: change a declaration and sync. lib.rs includes this file (and holds the adapters' modules)."
    echo
    echo "/// The Bevy minor these plugins were resolved for."
    echo "pub const BEVY: &str = \"$minor\";"
    echo
    echo "/// Registers every plugin this build has (by feature) with raea-render's hooks. Call before the first Engine or App."
    echo "pub fn install() {"
    cat "$reg"
    echo "}"
    echo
    echo "/// The declared plugins this build has: name, crate, the source resolved, the type-path prefixes the generator"
    echo "/// keeps, what a plain headless App needs for it, and whether only an App that names it gets it."
    echo "pub fn declared() -> Vec<Declared> {"
    echo "    #[allow(unused_mut)]"
    echo "    let mut v = Vec::new();"
    cat "$list"
    echo "    v"
    echo "}"
  } > "$DECLARED"
  rm -f "$deps" "$feats" "$cdeps" "$cfeats" "$adeps" "$afeats" "$reg" "$list" "$mods" "$cmods" "$amods"
  echo "bevy-plugins: synced $(jq -r 'keys | length' <<<"$all") declarations for Bevy $minor from ${#files[@]} file(s)"
  [ -z "$skipped" ] || echo "bevy-plugins: no release for Bevy $minor, left out:$skipped"
  [ -z "$own" ] || echo "bevy-plugins: raea's own, in every build (nothing to sync):$own"
}

decl_of() { # DIR -> its package.json
  local f=$1/package.json
  [ -f "$f" ] || die "$1 has no package.json"
  jq -e "$PLUGINS"' | length > 0' "$f" >/dev/null || die "$f declares no raea.plugins"
  # every reference resolves, checked here in the caller's shell (inside $(...) a die would only end the subshell)
  local name
  for name in $(jq -r "$PLUGINS"' | to_entries[] | select(.value | type == "object" and has("from")) | .key' "$f"); do
    spec_of "$f" "$name" >/dev/null
  done
  echo "$f"
}

# where DIR's generated modules go: raea.generated relative to DIR, else DIR/bevy
out_of() { # DIR
  local g; g=$(jq -r '.raea.generated // "bevy"' "$1/package.json")
  echo "$1/$g"
}

cmd_features() { # DIR
  local f; f=$(decl_of "$1")
  { echo bevy-plugins; cargo_names "$f" | sed 's|^|raea-bevy-plugins/|'; } | paste -sd, -
}

cmd_build() { # DIR
  local dir; dir=$(cd "$1" && pwd)
  cmd_sync
  local feats; feats=$(cmd_features "$dir")
  echo "bevy-plugins: building raea-bevy-sdk with $feats"
  (cd "$ROOT" && lane build -- cargo build --release -p raea-cli --bin raea-bevy-sdk --features "$feats" -j 6)
  if ! cmd_check "$dir" >/dev/null 2>&1; then
    echo "bevy-plugins: $dir's TypeScript is behind its plugins; regenerating"
    cmd_gen "$dir"
  fi
}

# the version Cargo.lock resolved for a crate (several: the one the plugin crate links)
locked_version() {
  (cd "$ROOT" && cargo tree -p raea-bevy-plugins --features "$2" -e normal --depth 1 --prefix none -f '{p}' 2>/dev/null) | awk -v c="$1" '$1 == c { print $2; exit }'
}

# raea's own plugin's handle, for one that names no types: what a plugin list takes, marked as raea's
raea_handle() { # NAME SPEC
  local note; note=$(jq -r '.note // empty' <<<"$2")
  printf '%s\n' "// Generated by tools/bevy-plugins.sh gen from $1's declaration (raea's own plugin, in every raea build); don't" \
    "// edit, regenerate." \
    "" \
    "/** $1${note:+: $note}. Named in a project's raea.plugins, it says the project uses it; a headless BevyApp can't open it. */" \
    "export const $1 = {" \
    "  plugin: \"$1\" as const," \
    "  raea: true as const," \
    "};"
}

cmd_gen() { # DIR [OUTDIR]
  local dir out f name spec crate prefixes args version hash g plugins='{}'
  dir=$(cd "$1" && pwd); out=${2:-$(out_of "$dir")}
  f=$(decl_of "$dir")
  [ -x "$SDK" ] || die "build first: tools/bevy-plugins.sh build $1"
  mkdir -p "$out"
  local schema; schema=$(mktemp -d)
  for name in $(released_names "$f"); do
    # a plugin from another package: its handle is that package's
    jq -e --arg n "$name" "$PLUGINS"'[$n] | type == "object" and has("from")' "$f" >/dev/null && { plugins=$(jq -c --arg n "$name" --arg p "$(jq -r --arg n "$name" "$PLUGINS"'[$n].from' "$f")" '.[$n] = {from: $p}' <<<"$plugins"); continue; }
    spec=$(jq -c --arg n "$name" "$PLUGINS"'[$n]' "$f")
    crate=$(jq -r --arg n "$name" '.crate // $n' <<<"$spec")
    # raea's own plugin naming no types: its handle alone, which names it in a plugin list
    if [ "$(jq -r '.raea == true and (.types | not)' <<<"$spec")" = true ]; then
      raea_handle "$name" "$spec" > "$out/$name.gen.tsx"
      plugins=$(jq -c --arg n "$name" '.[$n] = {raea: true}' <<<"$plugins")
      continue
    fi
    # a builtin is generated only when it names the types it brings (bevy's features do)
    [ "$(jq -r '(.builtin // false) and (.types | not)' <<<"$spec")" = true ] && { plugins=$(jq -c --arg n "$name" --arg c "$crate" --arg v "$(locked_version "$crate" "")" '.[$n] = {crate: $c, version: $v, builtin: true}' <<<"$plugins"); continue; }
    args=()
    while read -r p; do args+=(--prefix "$p"); done < <(jq -r --arg c "${crate//-/_}" '.types // ["\($c)::"] | .[]' <<<"$spec")
    "$SDK" "${args[@]}" --plugin "$name" --title "$name ($crate)" -o "$out/$name.gen.tsx" --schema "$schema/$name.json" 2> "$schema/$name.log" || { cat "$schema/$name.log" >&2; die "generating $name failed"; }
    tail -1 "$schema/$name.log"
    hash=$(shasum -a 256 "$schema/$name.json" | cut -c1-16)
    if jq -e '.raea == true' <<<"$spec" >/dev/null; then
      # raea's own: in every build, no feature of its name; marked in its handle, which no headless App can open
      perl -0pi -e 's/^  plugin: ("[a-z0-9_]+") as const,$/  plugin: $1 as const,\n  raea: true as const,/m' "$out/$name.gen.tsx"
      plugins=$(jq -c --arg n "$name" --arg c "$crate" --arg h "$hash" '.[$n] = {crate: $c, raea: true, schema: $h}' <<<"$plugins")
      continue
    fi
    version=$(locked_version "$crate" "$name")
    plugins=$(jq -c --arg n "$name" --arg c "$crate" --arg v "$version" --arg h "$hash" '.[$n] = {crate: $c, version: $v, schema: $h}' <<<"$plugins")
  done
  rm -rf "$schema"
  jq -n --arg b "$(bevy_minor)" --argjson p "$plugins" '{bevy: $b, plugins: $p}' > "$out/plugins.lock.json"
  # a package's folder (raea.generated): an index of every generated plugin's handle, by its name
  if jq -e '.raea.generated' "$f" >/dev/null; then
    {
      echo "// Generated by tools/bevy-plugins.sh gen from ${f#"$ROOT"/}; don't edit, regenerate. Each plugin's handle,"
      echo "// by name; its wrappers and types are raea/plugins/<name>."
      for g in "$out"/*.gen.tsx; do
        name=$(basename "$g" .gen.tsx)
        # the handle is the name in camelCase (raea-bevy-sdk: big_space is bigSpace)
        echo "export { $(perl -pe 's/_([a-z0-9])/\U$1/g' <<<"$name") } from \"./$name.gen\";"
      done
    } > "$out/index.ts"
  fi
  echo "bevy-plugins: generated $(jq -r 'keys | join(", ")' <<<"$plugins") into ${out#"$ROOT"/}"
}

cmd_check() { # DIR
  local dir gen lock tmp status=0 name want have
  dir=$(cd "$1" && pwd); gen=$(out_of "$dir"); lock=$gen/plugins.lock.json
  [ -f "$lock" ] || die "$dir has no ${lock#"$dir"/}: run tools/bevy-plugins.sh build $1"
  [ "$(jq -r .bevy "$lock")" = "$(bevy_minor)" ] || { echo "drift: $lock is for Bevy $(jq -r .bevy "$lock"), the tree is on $(bevy_minor)"; status=1; }
  tmp=$(mktemp -d)
  cmd_gen "$dir" "$tmp" >/dev/null
  for name in $(jq -r '.plugins | keys[]' "$lock"); do
    want=$(jq -r --arg n "$name" '.plugins[$n] | if . == null then "missing" else (.version // "unversioned") end' "$lock")
    have=$(jq -r --arg n "$name" '.plugins[$n] | if . == null then "missing" else (.version // "unversioned") end' "$tmp/plugins.lock.json")
    [ "$want" = "$have" ] || { echo "drift: $name is $have in the build, $want in $lock"; status=1; }
    if [ -f "$tmp/$name.gen.tsx" ] && ! cmp -s "$tmp/$name.gen.tsx" "$gen/$name.gen.tsx"; then
      echo "drift: $name's generated TypeScript differs from the build's registry:"; diff "$gen/$name.gen.tsx" "$tmp/$name.gen.tsx" | head -20; status=1
    fi
  done
  for name in $(jq -r '.plugins | keys[]' "$tmp/plugins.lock.json"); do
    jq -e --arg n "$name" '.plugins[$n]' "$lock" >/dev/null || { echo "drift: $name is declared but not in $lock"; status=1; }
  done
  if [ -f "$tmp/index.ts" ] && ! cmp -s "$tmp/index.ts" "$gen/index.ts"; then echo "drift: ${gen#"$dir"/}/index.ts isn't the generated plugins' index"; status=1; fi
  rm -rf "$tmp"
  [ $status = 0 ] && echo "bevy-plugins: $1 matches its lock" || echo "bevy-plugins: $1 drifted; run tools/bevy-plugins.sh build $1 (or gen) and review the diff"
  return $status
}

case "${1:-}" in
  sync) shift; cmd_sync "$@" ;;
  build) [ $# = 2 ] || die "build DIR"; cmd_build "$2" ;;
  gen) [ $# -ge 2 ] || die "gen DIR"; cmd_gen "$2" "${3:-}" ;;
  check) [ $# = 2 ] || die "check DIR"; cmd_check "$2" ;;
  features) [ $# = 2 ] || die "features DIR"; cmd_features "$2" ;;
  *) sed -n '2,41p' "$0" | sed 's/^# \{0,1\}//'; exit 2 ;;
esac
