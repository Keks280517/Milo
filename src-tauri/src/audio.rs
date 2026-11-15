//! Ausgabegeraete: welche es gibt, welches gerade spielt, und Umschalten auf ein anderes.
//!
//! Windows bietet zum Umschalten des Standardgeraets keine offizielle Schnittstelle. Alle bekannten
//! Werkzeuge (auch die Windows-Einstellungen selbst) benutzen dafuer `IPolicyConfig`, eine seit
//! Windows 7 unveraenderte COM-Schnittstelle. Schlaegt sie fehl, oeffnet Milo die Sound-Einstellungen.

use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OutputDevice {
    pub id: String,
    pub name: String,
    pub default: bool,
}

/// Alle aktiven Ausgabegeraete; das Standardgeraet ist markiert.
#[tauri::command]
pub async fn audio_outputs() -> Result<Vec<OutputDevice>, String> {
    tauri::async_runtime::spawn_blocking(native::outputs).await.map_err(|e| e.to_string())?.map_err(|e| e.to_string())
}

/// Macht `id` zum Standard-Ausgabegeraet (fuer Musik, Spiele und Gespraeche).
#[tauri::command]
pub async fn audio_set_output(id: String) -> Result<Vec<OutputDevice>, String> {
    let known = audio_outputs().await?;
    if !known.iter().any(|d| d.id == id) {
        return Err(tr!("Dieses Gerät gibt es nicht mehr.", "That device doesn't exist anymore.").into());
    }
    tauri::async_runtime::spawn_blocking(move || native::set_default(&id)).await.map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;
    audio_outputs().await
}

#[cfg(windows)]
#[allow(non_snake_case)]
mod native {
    use super::OutputDevice;
    use anyhow::Result;
    use std::ffi::c_void;
    use windows::core::{interface, IUnknown, IUnknown_Vtbl, GUID, HRESULT, PCWSTR, PWSTR};
    use windows::Win32::Foundation::PROPERTYKEY;
    use windows::Win32::Media::Audio::{eConsole, eMultimedia, eCommunications, eRender, ERole, IMMDeviceEnumerator, MMDeviceEnumerator, DEVICE_STATE_ACTIVE};
    use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_MULTITHREADED, STGM_READ};

    /// PKEY_Device_FriendlyName (aus functiondiscoverykeys_devpkey.h).
    const FRIENDLY_NAME: PROPERTYKEY = PROPERTYKEY { fmtid: GUID::from_u128(0xa45c254e_df1c_4efd_8020_67d146a850e0), pid: 14 };
    const POLICY_CONFIG_CLIENT: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);

    #[interface("f8679f50-850a-41cf-9c72-430f290290c8")]
    unsafe trait IPolicyConfig: IUnknown {
        fn GetMixFormat(&self, device: PCWSTR, format: *mut *mut c_void) -> HRESULT;
        fn GetDeviceFormat(&self, device: PCWSTR, default: i32, format: *mut *mut c_void) -> HRESULT;
        fn ResetDeviceFormat(&self, device: PCWSTR) -> HRESULT;
        fn SetDeviceFormat(&self, device: PCWSTR, endpoint: *mut c_void, mix: *mut c_void) -> HRESULT;
        fn GetProcessingPeriod(&self, device: PCWSTR, default: i32, period: *mut i64, minimum: *mut i64) -> HRESULT;
        fn SetProcessingPeriod(&self, device: PCWSTR, period: *mut i64) -> HRESULT;
        fn GetShareMode(&self, device: PCWSTR, mode: *mut c_void) -> HRESULT;
        fn SetShareMode(&self, device: PCWSTR, mode: *mut c_void) -> HRESULT;
        fn GetPropertyValue(&self, device: PCWSTR, key: *const c_void, value: *mut c_void) -> HRESULT;
        fn SetPropertyValue(&self, device: PCWSTR, key: *const c_void, value: *mut c_void) -> HRESULT;
        fn SetDefaultEndpoint(&self, device: PCWSTR, role: ERole) -> HRESULT;
        fn SetEndpointVisibility(&self, device: PCWSTR, visible: i32) -> HRESULT;
    }

    unsafe fn take(text: PWSTR) -> String {
        unsafe {
            let value = text.to_string().unwrap_or_default();
            CoTaskMemFree(Some(text.0 as *const c_void));
            value
        }
    }

    pub fn outputs() -> Result<Vec<OutputDevice>> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let devices: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            let default = devices.GetDefaultAudioEndpoint(eRender, eConsole).and_then(|d| d.GetId()).map(|id| take(id)).unwrap_or_default();
            let list = devices.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)?;
            let mut out = Vec::new();
            for i in 0..list.GetCount()? {
                let device = list.Item(i)?;
                let id = take(device.GetId()?);
                let name = device
                    .OpenPropertyStore(STGM_READ)
                    .and_then(|store| store.GetValue(&FRIENDLY_NAME))
                    .and_then(|value| PropVariantToStringAlloc(&value))
                    .map(|text| take(text))
                    .unwrap_or_else(|_| tr!("Ausgabegerät", "Output device").into());
                out.push(OutputDevice { default: id == default, id, name });
            }
            Ok(out)
        }
    }

    pub fn set_default(id: &str) -> Result<()> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let policy: IPolicyConfig = CoCreateInstance(&POLICY_CONFIG_CLIENT, None, CLSCTX_ALL)?;
            let wide: Vec<u16> = id.encode_utf16().chain(std::iter::once(0)).collect();
            for role in [eConsole, eMultimedia, eCommunications] {
                policy.SetDefaultEndpoint(PCWSTR(wide.as_ptr()), role).ok()?;
            }
            Ok(())
        }
    }
}

#[cfg(not(windows))]
mod native {
    use super::OutputDevice;
    pub fn outputs() -> anyhow::Result<Vec<OutputDevice>> {
        Ok(Vec::new())
    }
    pub fn set_default(_id: &str) -> anyhow::Result<()> {
        anyhow::bail!(tr!("Nur unter Windows verfügbar.", "Only available on Windows."))
    }
}
