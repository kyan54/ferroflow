//! Windows system-proxy via `HKCU\...\Internet Settings` + `InternetSetOption`
//! broadcast (`INTERNET_OPTION_SETTINGS_CHANGED`/`INTERNET_OPTION_REFRESH`)
//! so running apps (Edge/IE-based) pick up the change without a reboot.
