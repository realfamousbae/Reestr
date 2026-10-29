//! Files the phone exchanges with the computer through Yandex Disk (D39). On Android a file
//! picked in the system picker is a `content://` address that the app may read only while it
//! runs, unless it asks to keep the access: this plugin picks with the access kept and copies
//! between such addresses and the app's own files. On the computer the addresses are paths.

use std::path::Path;

use serde::{Deserialize, Serialize};
use tauri::plugin::{Builder, TauriPlugin};
use tauri::{Manager, Runtime};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("the picker is only on Android")]
    NoPicker,
    #[error("android: {0}")]
    Android(String),
}

/// A picked file: its address and the name the user saw.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Picked {
    pub uri: String,
    pub name: Option<String>,
}

#[cfg(target_os = "android")]
mod android {
    use std::path::Path;

    use serde::{Deserialize, Serialize};
    use tauri::Runtime;
    use tauri::plugin::PluginHandle;

    use crate::{Error, Picked};

    #[derive(Deserialize)]
    struct PickResult {
        uri: Option<String>,
        name: Option<String>,
    }

    #[derive(Serialize)]
    struct CopyArgs<'a> {
        uri: &'a str,
        path: &'a str,
    }

    pub struct Files<R: Runtime>(pub PluginHandle<R>);

    fn path_text(path: &Path) -> Result<&str, Error> {
        path.to_str()
            .ok_or_else(|| Error::Android("path is not UTF-8".into()))
    }

    impl<R: Runtime> Files<R> {
        pub fn pick(&self) -> Result<Option<Picked>, Error> {
            let r: PickResult = self
                .0
                .run_mobile_plugin("pick", ())
                .map_err(|e| Error::Android(e.to_string()))?;
            Ok(r.uri.map(|uri| Picked { uri, name: r.name }))
        }

        pub fn copy_from(&self, uri: &str, to: &Path) -> Result<(), Error> {
            let args = CopyArgs {
                uri,
                path: path_text(to)?,
            };
            self.0
                .run_mobile_plugin::<()>("copyFrom", args)
                .map_err(|e| Error::Android(e.to_string()))
        }

        pub fn copy_to(&self, from: &Path, uri: &str) -> Result<(), Error> {
            let args = CopyArgs {
                uri,
                path: path_text(from)?,
            };
            self.0
                .run_mobile_plugin::<()>("copyTo", args)
                .map_err(|e| Error::Android(e.to_string()))
        }
    }
}

#[cfg(not(target_os = "android"))]
mod desktop {
    use std::marker::PhantomData;
    use std::path::Path;

    use tauri::Runtime;

    use crate::{Error, Picked};

    pub struct Files<R: Runtime>(pub PhantomData<fn() -> R>);

    impl<R: Runtime> Files<R> {
        pub fn pick(&self) -> Result<Option<Picked>, Error> {
            Err(Error::NoPicker)
        }

        pub fn copy_from(&self, uri: &str, to: &Path) -> Result<(), Error> {
            std::fs::copy(uri, to)?;
            Ok(())
        }

        pub fn copy_to(&self, from: &Path, uri: &str) -> Result<(), Error> {
            std::fs::copy(from, uri)?;
            Ok(())
        }
    }
}

#[cfg(target_os = "android")]
use android::Files as Inner;
#[cfg(not(target_os = "android"))]
use desktop::Files as Inner;

/// The plugin's handle, reached through [`FilesExt::files`].
pub struct Files<R: Runtime>(Inner<R>);

impl<R: Runtime> Files<R> {
    /// The system picker; the picked file stays readable and writable after a restart.
    /// `None` when the user cancels. Android only.
    pub fn pick(&self) -> Result<Option<Picked>, Error> {
        self.0.pick()
    }

    /// Copies the file at `uri` into `to`, a file of the app.
    pub fn copy_from(&self, uri: &str, to: &Path) -> Result<(), Error> {
        self.0.copy_from(uri, to)
    }

    /// Replaces the content of the file at `uri` with `from`, a file of the app.
    pub fn copy_to(&self, from: &Path, uri: &str) -> Result<(), Error> {
        self.0.copy_to(from, uri)
    }
}

pub trait FilesExt<R: Runtime> {
    fn files(&self) -> &Files<R>;
}

impl<R: Runtime, T: Manager<R>> FilesExt<R> for T {
    fn files(&self) -> &Files<R> {
        self.state::<Files<R>>().inner()
    }
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("reestr-files")
        .setup(|app, api| {
            #[cfg(target_os = "android")]
            let inner =
                android::Files(api.register_android_plugin("app.reestr.files", "FilesPlugin")?);
            #[cfg(not(target_os = "android"))]
            let inner = {
                let _ = api;
                desktop::Files::<R>(std::marker::PhantomData)
            };
            app.manage(Files(inner));
            Ok(())
        })
        .build()
}
