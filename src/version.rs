#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ReleaseVersion {
    year: u16,
    month: u8,
    day: u8,
    build: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DottedVersion(Vec<u64>);

pub(crate) const BUILD_TARGET: &str = env!("TPM_BUILD_TARGET");
pub(crate) const DISPLAY_VERSION: &str = env!("TPM_BUILD_VERSION");

pub(crate) enum VersionStatus {
    Same,
    NewerAvailable,
    CurrentIsNewer,
}

pub(crate) fn compare_available_version(current: &str, latest: &str) -> VersionStatus {
    if current == latest {
        return VersionStatus::Same;
    }

    match (
        parse_release_version(current),
        parse_release_version(latest),
    ) {
        (Some(current), Some(latest)) => compare_ordered(&current, &latest),
        (None, Some(_)) => VersionStatus::NewerAvailable,
        (Some(_), None) => VersionStatus::CurrentIsNewer,
        (None, None) => match (parse_dotted_version(current), parse_dotted_version(latest)) {
            (Some(current), Some(latest)) => compare_ordered(&current, &latest),
            _ => VersionStatus::NewerAvailable,
        },
    }
}

fn parse_release_version(input: &str) -> Option<ReleaseVersion> {
    let (date, build) = input.rsplit_once('-')?;
    let mut date_parts = date.split('.');

    let year = date_parts.next()?.parse().ok()?;
    let month = date_parts.next()?.parse().ok()?;
    let day = date_parts.next()?.parse().ok()?;

    if date_parts.next().is_some() {
        return None;
    }

    let build = build.parse().ok()?;

    Some(ReleaseVersion {
        year,
        month,
        day,
        build,
    })
}

fn parse_dotted_version(input: &str) -> Option<DottedVersion> {
    let input = input.trim().strip_prefix('v').unwrap_or(input.trim());
    let parts = input
        .split('.')
        .map(str::parse)
        .collect::<std::result::Result<Vec<u64>, _>>()
        .ok()?;

    if parts.is_empty() {
        None
    } else {
        Some(DottedVersion(parts))
    }
}

fn compare_ordered<T>(current: &T, latest: &T) -> VersionStatus
where
    T: Ord,
{
    match latest.cmp(current) {
        std::cmp::Ordering::Greater => VersionStatus::NewerAvailable,
        std::cmp::Ordering::Equal => VersionStatus::Same,
        std::cmp::Ordering::Less => VersionStatus::CurrentIsNewer,
    }
}

impl PartialOrd for DottedVersion {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DottedVersion {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let max_len = self.0.len().max(other.0.len());

        for index in 0..max_len {
            let left = self.0.get(index).copied().unwrap_or(0);
            let right = other.0.get(index).copied().unwrap_or(0);

            match left.cmp(&right) {
                std::cmp::Ordering::Equal => continue,
                ordering => return ordering,
            }
        }

        std::cmp::Ordering::Equal
    }
}

#[cfg(test)]
mod tests {
    use super::{VersionStatus, compare_available_version};

    #[test]
    fn treats_newer_release_tags_as_newer() {
        assert!(matches!(
            compare_available_version("2026.04.03-12", "2026.04.04-1"),
            VersionStatus::NewerAvailable
        ));
    }

    #[test]
    fn treats_matching_versions_as_same() {
        assert!(matches!(
            compare_available_version("0.1.0", "0.1.0"),
            VersionStatus::Same
        ));
    }

    #[test]
    fn treats_release_tags_as_newer_than_non_release_builds() {
        assert!(matches!(
            compare_available_version("0.1.0", "2026.04.04-1"),
            VersionStatus::NewerAvailable
        ));
    }

    #[test]
    fn compares_dotted_versions_component_wise() {
        assert!(matches!(
            compare_available_version("0.10.0", "0.2.0"),
            VersionStatus::CurrentIsNewer
        ));
    }

    #[test]
    fn compares_v_prefixed_dotted_versions() {
        assert!(matches!(
            compare_available_version("v1.2.3", "v1.10.0"),
            VersionStatus::NewerAvailable
        ));
    }
}
