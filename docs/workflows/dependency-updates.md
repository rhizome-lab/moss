# Dependency Updates Workflow

Keeping dependencies current: security patches, bug fixes, new features.

## Trigger

- Security vulnerability announced
- Dependabot/Renovate PR opened
- Regular maintenance schedule
- New feature requires updated dependency

## Goal

- Update dependencies safely
- Maintain compatibility
- Avoid breaking changes
- Document any required code changes

## Prerequisites

- Tests passing on current version
- Understanding of semver in use
- Access to changelogs/release notes

## Decomposition Strategy

**Audit → Evaluate → Update → Verify**

```
1. AUDIT: Assess current state
   - List outdated dependencies
   - Check for security advisories
   - Identify update priorities

2. EVALUATE: Understand each update
   - Read changelog/release notes
   - Check for breaking changes
   - Assess risk level

3. UPDATE: Apply changes
   - Update one or batch carefully
   - Handle breaking API changes
   - Update lock files

4. VERIFY: Ensure nothing broke
   - Run test suite
   - Check for deprecation warnings
   - Smoke test key functionality
```

## Tools Used

| Phase | Tools |
|-------|-------|
| Audit | `moss package outdated`, `cargo audit` |
| Evaluate | Changelogs, release notes, `view` |
| Update | `cargo update`, `npm update`, etc. |
| Verify | Test suite, `moss lint`, manual testing |

## Update Strategies

### Patch Updates (x.y.Z)
```
Low risk: bug fixes, no API changes
→ Batch and update together
→ Run tests, should pass
```

### Minor Updates (x.Y.z)
```
Medium risk: new features, deprecations possible
→ Update individually or small batches
→ Check for deprecation warnings
→ Review changelog
```

### Major Updates (X.y.z)
```
High risk: breaking changes expected
→ Update one at a time
→ Read migration guide
→ Plan code changes
→ Thorough testing
```

## Priority Order

1. **Security vulnerabilities** - Update immediately
2. **Bug fixes affecting you** - Update soon
3. **Major versions behind** - Plan update
4. **Minor updates** - Regular maintenance
5. **Patch updates** - Batch with other work

## Failure Modes

| Failure | Detection | Recovery |
|---------|-----------|----------|
| Breaking change missed | Tests fail, runtime errors | Read changelog, fix or rollback |
| Transitive dependency conflict | Build fails | Pin versions, use overrides |
| Subtle behavior change | Tests pass but wrong behavior | Add tests for specific behavior |
| Too many updates at once | Hard to debug failures | Bisect or rollback to smaller batch |

## Example Session

**Goal**: Update dependencies in a Rust project

```
Turn 1: Audit outdated packages
  $(moss package outdated)
  → serde 1.0.190 → 1.0.195 (patch)
  → tokio 1.32.0 → 1.35.0 (minor)
  → axum 0.6.20 → 0.7.4 (major)
  → 3 security advisories

Turn 2: Check security advisories
  $(cargo audit)
  → RUSTSEC-2024-XXX: Update serde immediately

Turn 3: Apply patch updates
  $(cargo update -p serde)
  → serde updated

Turn 4: Evaluate minor update
  → tokio 1.32→1.35 changelog: new features, no breaking changes
  $(cargo update -p tokio)
  → Run tests: pass

Turn 5: Evaluate major update
  → axum 0.6→0.7: Multiple breaking changes
  → Router API changed
  → Handler trait changed
  → Plan: Read migration guide, update incrementally

Turn 6: Verify
  $(cargo test)
  $(cargo clippy)
  → All passing, no warnings
```

## Major Version Update Process

```
1. Read migration guide completely
2. Create tracking issue for update
3. Make breaking changes in feature branch
4. Update incrementally:
   a. Update dependency version
   b. Fix compile errors
   c. Fix test failures
   d. Check for behavior changes
5. Review all changes before merge
6. Test in staging environment
```

## Lockfile Management

### Lockfile Checked In
- Reproducible builds
- Update explicitly with `cargo update`
- Review lockfile changes in PRs

### Lockfile Not Checked In
- Fresh resolution each build
- May get unexpected updates
- Pin important versions explicitly

## Anti-patterns

- **Update and pray**: Updating without reading changelog
- **Fear of updating**: Staying on old versions indefinitely
- **All at once**: Updating everything in one PR
- **Ignoring deprecations**: Not fixing until forced

## Automation

Consider automated dependency updates:
- Dependabot (GitHub)
- Renovate
- cargo-outdated in CI

But always review before merging.

## See Also

- [Breaking API Changes](breaking-api-changes.md) - Handling upstream breaking changes
- [Migration](migration.md) - Larger migration workflows
