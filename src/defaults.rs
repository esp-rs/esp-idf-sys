use crate::bindings::*;

impl Default for wifi_init_config_t {
    fn default() -> Self {
        Self {
            event_handler: Some(esp_event_send_internal),
            osi_funcs: unsafe { &mut g_wifi_osi_funcs },
            wpa_crypto_funcs: unsafe { g_wifi_default_wpa_crypto_funcs },
            static_rx_buf_num: CONFIG_ESP32_WIFI_STATIC_RX_BUFFER_NUM as _,
            dynamic_rx_buf_num: CONFIG_ESP32_WIFI_DYNAMIC_RX_BUFFER_NUM as _,
            tx_buf_type: CONFIG_ESP32_WIFI_TX_BUFFER_TYPE as _,
            static_tx_buf_num: WIFI_STATIC_TX_BUFFER_NUM as _,
            dynamic_tx_buf_num: WIFI_DYNAMIC_TX_BUFFER_NUM as _,
            cache_tx_buf_num: WIFI_CACHE_TX_BUFFER_NUM as _,
            csi_enable: WIFI_CSI_ENABLED as _,
            ampdu_rx_enable: WIFI_AMPDU_RX_ENABLED as _,
            ampdu_tx_enable: WIFI_AMPDU_TX_ENABLED as _,
            amsdu_tx_enable: WIFI_AMSDU_TX_ENABLED as _,
            nvs_enable: WIFI_NVS_ENABLED as _,
            nano_enable: WIFI_NANO_FORMAT_ENABLED as _,
            rx_ba_win: WIFI_DEFAULT_RX_BA_WIN as _,
            wifi_task_core_id: WIFI_TASK_CORE_ID as _,
            beacon_max_len: WIFI_SOFTAP_BEACON_MAX_LEN as _,
            mgmt_sbuf_num: WIFI_MGMT_SBUF_NUM as _,
            feature_caps: unsafe { g_wifi_feature_caps },
            sta_disconnected_pm: WIFI_STA_DISCONNECTED_PM_ENABLED != 0,
            magic: WIFI_INIT_CONFIG_MAGIC as _,
        }
    }
}

impl Default for esp_log_level_t {
    fn default() -> Self {
        let default_log_level = esp_log_level_t(CONFIG_LOG_DEFAULT_LEVEL);

        if default_log_level >= esp_log_level_t::ESP_LOG_VERBOSE {
            esp_log_level_t::ESP_LOG_VERBOSE
        } else if default_log_level >= esp_log_level_t::ESP_LOG_DEBUG {
            esp_log_level_t::ESP_LOG_DEBUG
        } else if default_log_level >= esp_log_level_t::ESP_LOG_INFO {
            esp_log_level_t::ESP_LOG_INFO
        } else if default_log_level >= esp_log_level_t::ESP_LOG_WARN {
            esp_log_level_t::ESP_LOG_WARN
        } else if default_log_level >= esp_log_level_t::ESP_LOG_ERROR {
            esp_log_level_t::ESP_LOG_ERROR
        } else {
            esp_log_level_t::ESP_LOG_NONE
        }
    }
}
