p = 'src/confirmation.rs'
s = open(p).read()
s = s.replace('    pub fn from_str(s: &str) -> Option<Self> {',
              '    /// Parse a reason name; kept as an inherent method to mirror the\n    /// original\'s string union without pulling in `FromStr`.\n    #[allow(clippy::should_implement_trait)]\n    pub fn parse(s: &str) -> Option<Self> {')
open(p, 'w').write(s)

t = open('tests/confirmation.rs').read()
t = t.replace('DenialReason::from_str(reason_str)', 'DenialReason::parse(reason_str)')
open(t, 'w').write(t)
print('renamed')
