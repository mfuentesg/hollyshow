//! Strongly typed string identifiers shared by every crate and (via ts-rs) by TypeScript.

use serde::{Deserialize, Serialize};
use std::fmt;
use ts_rs::TS;

macro_rules! id_type {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
        #[ts(export)]
        pub struct $name(pub String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self(value.to_owned())
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self(value)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

id_type!(
    /// Stable identifier of a song in the library.
    SongId
);
id_type!(
    /// Identifier of a song section (verse, chorus...). Unique inside its song.
    SectionId
);
id_type!(
    /// Identifier of a song arrangement.
    ArrangementId
);
id_type!(
    /// Identifier of a service (playlist).
    PlaylistId
);
id_type!(
    /// Identifier of an item inside a service. Also identifies the matching [`crate::ShowEntry`].
    ItemId
);
id_type!(
    /// Identifier of a theme.
    ThemeId
);
id_type!(
    /// Identifier of a media asset (image, video, audio).
    MediaId
);
id_type!(
    /// Identifier of a slide, unique inside its show entry.
    SlideId
);
id_type!(
    /// Identifier of a physical or virtual output (`main`, `stage`, `lobby`, `obs`...).
    OutputId
);
id_type!(
    /// Client-generated command identifier used for idempotency and acks.
    CommandId
);
id_type!(
    /// Identifier of a timer.
    TimerId
);
id_type!(
    /// Identifier of a paired remote device.
    DeviceId
);
