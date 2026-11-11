use crate::abi::Status;
use crate::services::Service;

use super::{Load, Off, ok, service_spec};

impl BaseLoad {
    /// As `Load::publish`.
    pub fn publish<T: Service>(&mut self, table: &'static T) -> Result<(), Off> {
        let spec = service_spec::<T>(core::ptr::from_ref(table).cast());
        let status = (self.api().publish)(self.host.handle(), &raw const spec);
        ok(status, (), || {
            format!(
                "service {} v{}: published by another mod",
                T::NAME,
                T::MAJOR
            )
        })
    }

    /// As `Load::service`.
    pub fn service<T: Service>(&self) -> Result<&'static T, Off> {
        self.find_service::<T>()
            .map_err(|refused| Off::new(refused.to_string()))
    }

    /// As `Load::optional_service`.
    #[must_use]
    pub fn optional_service<T: Service>(&self) -> Option<&'static T> {
        match self.find_service::<T>() {
            Ok(table) => Some(table),
            Err(refused) => {
                let level = if refused.status() == Status::MISSING {
                    crate::abi::Level::DEBUG
                } else {
                    crate::abi::Level::WARN
                };
                self.host.log(level, &refused.to_string());
                None
            }
        }
    }

    fn find_service<T: Service>(&self) -> Result<&'static T, FindServiceError> {
        let spec = service_spec::<T>(core::ptr::null());
        let mut found = crate::abi::ServiceFound::EMPTY;
        let status = (self.api().service)(self.host.handle(), &raw const spec, &raw mut found);
        if !status.is_ok() {
            // SAFETY: the loader keeps the reason for the process.
            let reason = unsafe { found.reason.as_str() }.unwrap_or_default();
            return Err(FindServiceError::Refused {
                name: T::NAME,
                major: T::MAJOR,
                status,
                reason: reason.to_owned(),
            });
        }
        // SAFETY: the loader checked the published layout starts with
        // every field of `T`, and tables are `'static`.
        unsafe { found.table.cast::<T>().as_ref() }.ok_or(FindServiceError::NullTable {
            name: T::NAME,
            major: T::MAJOR,
        })
    }
}

/// Why a lookup of a service found no table: the loader's status and
/// reason, or a table the loader handed over as null.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
enum FindServiceError {
    /// The loader refused the lookup with `status`; `reason` is empty when
    /// it gave none.
    #[error("service {name} v{major}: {}", if reason.is_empty() { "not found" } else { reason.as_str() })]
    Refused {
        name: &'static str,
        major: u32,
        status: Status,
        reason: String,
    },
    /// The loader found the service but handed over no table.
    #[error("service {name} v{major}: null table")]
    NullTable { name: &'static str, major: u32 },
}

impl FindServiceError {
    /// The status of the lookup: the loader's, or missing for a null table.
    const fn status(&self) -> Status {
        match self {
            Self::Refused { status, .. } => *status,
            Self::NullTable { .. } => Status::MISSING,
        }
    }
}
