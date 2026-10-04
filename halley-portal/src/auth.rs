//! Bind portal requests and sessions to their initiating D-Bus connection.
use zbus::fdo;
use zbus::names::{OwnedUniqueName, UniqueName};

pub fn sender(sender: Option<&UniqueName<'_>>) -> fdo::Result<OwnedUniqueName> {
    sender.map(|name| name.to_owned().into()).ok_or_else(denied)
}

pub fn same_owner(sender: &str, owner: &str) -> fdo::Result<()> {
    if sender.starts_with(':') && sender == owner {
        Ok(())
    } else {
        Err(denied())
    }
}

fn denied() -> fdo::Error {
    fdo::Error::AccessDenied("portal object belongs to another D-Bus connection".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_missing_replaced_and_well_known_owners() {
        assert!(same_owner(":1.2", ":1.2").is_ok());
        assert!(same_owner(":1.3", ":1.2").is_err());
        assert!(same_owner("", "").is_err());
        assert!(
            same_owner(
                "org.freedesktop.portal.Desktop",
                "org.freedesktop.portal.Desktop"
            )
            .is_err()
        );
        assert!(sender(None).is_err());
    }
}
