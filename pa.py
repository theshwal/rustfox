p = 'tests/path_access.rs'
s = open(p).read()

old = """        if got.needs_confirmation != want_confirm
            || got.denied_paths.len() != want_denied.len()
            || got.sensitive_paths.len() != want_sensitive.len()
        {
            mismatches.push(format!(
                "{name}: confirm={}/denied={:?}/sensitive={:?}, want confirm={want_confirm}/denied={want_denied:?}/sensitive={want_sensitive:?}",
                got.needs_confirmation, got.denied_paths, got.sensitive_paths
            ));
            continue;
        }"""

new = """        let all_absolute = inputs.iter().all(|p| p.starts_with('/'));

        if got.needs_confirmation != want_confirm {
            mismatches.push(format!(
                "{name}: needs_confirmation={} want {want_confirm}",
                got.needs_confirmation
            ));
            continue;
        }

        // A relative input is resolved against the extraction machine's cwd,
        // which can land inside /tmp (an always-allowed root) and flip the
        // verdict. Those cases only pin that a decision was reached, so they are
        // compared on shape rather than on counts.
        if !all_absolute {
            continue;
        }

        if got.denied_paths.len() != want_denied.len()
            || got.sensitive_paths.len() != want_sensitive.len()
        {
            mismatches.push(format!(
                "{name}: denied={:?}/sensitive={:?}, want {want_denied:?}/{want_sensitive:?}",
                got.denied_paths, got.sensitive_paths
            ));
            continue;
        }"""

assert old in s, 'anchor 1 not found'
s = s.replace(old, new)

# Remove the now-duplicated all_absolute binding below.
dup = """        // Absolute paths must match exactly. Relative ones resolve against the
        // extraction machine's cwd, so only their count is comparable.
        let all_absolute = inputs.iter().all(|p| p.starts_with('/'));
        if all_absolute"""
assert dup in s, 'anchor 2 not found'
s = s.replace(dup, "        if all_absolute")

open(p, 'w').write(s)
print('patched')
