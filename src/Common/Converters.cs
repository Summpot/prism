using System;
using System.Globalization;
using Avalonia;
using Avalonia.Data.Converters;
using Avalonia.Media;

namespace Prism.Common;

public static class Converters
{
    public static readonly IValueConverter BooleanNegation =
        new FuncValueConverter<bool, bool>(b => !b);

    public static readonly IValueConverter EqualsZero =
        new FuncValueConverter<int, bool>(c => c == 0);

    public static readonly IValueConverter NotEqualsZero =
        new FuncValueConverter<int, bool>(c => c != 0);

    public static readonly IValueConverter NullToBoolean =
        new FuncValueConverter<object?, bool>(o => o != null);

    public static readonly IValueConverter NotNullToBoolean = NullToBoolean;

    public static readonly IValueConverter NullToTrue =
        new FuncValueConverter<object?, bool>(o => o == null);

    public static readonly IValueConverter NullOrEmptyToBoolean =
        new FuncValueConverter<string?, bool>(s => !string.IsNullOrWhiteSpace(s));

    public static readonly IValueConverter SettingsCountLabel =
        new FuncValueConverter<int, string>(c =>
            Prism.I18n.I18nText.Format("middleware_settings_count", ("count", c)));

    public static readonly IValueConverter BoolToCopyOrCheckIcon =
        new FuncValueConverter<bool, Geometry>(copied => copied ? AppIcons.Check : AppIcons.Copy);

    public static readonly IValueConverter BoolToCopyOrCheckBrush =
        new FuncValueConverter<bool, IBrush>(copied =>
            copied ? ThemeBrush("SuccessColor", "#10b981") : ThemeBrush("MutedForegroundColor", "#9ca3af"));

    public static readonly IValueConverter StatusToBrush =
        new FuncValueConverter<bool, IBrush>(running =>
            running ? ThemeBrush("SuccessColor", "#10b981") : ThemeBrush("MutedForegroundColor", "#6b7280"));

    public static readonly IValueConverter RouteEquals =
        new FuncValueConverter<string?, string, bool>((current, target) =>
            string.Equals(current, target, StringComparison.OrdinalIgnoreCase));

    public static readonly IValueConverter TokenTypeLabel =
        new FuncValueConverter<string?, string>(value => value?.ToLowerInvariant() switch
        {
            "admin" => Prism.I18n.I18nText.T("users_token_type_admin"),
            "connector" => Prism.I18n.I18nText.T("users_token_type_connector"),
            "client" => Prism.I18n.I18nText.T("users_token_type_client"),
            _ => value ?? ""
        });

    public static readonly IValueConverter ExpandedSidebarWidth =
        new FuncValueConverter<bool, double>(expanded => expanded ? 240 : 56);

    public static readonly IValueConverter IntEquals =
        new FuncValueConverter<int, string, bool>((val, param) =>
            int.TryParse(param, out var target) && val == target);

    public static readonly IValueConverter LogLevelToBrush =
        new FuncValueConverter<string?, IBrush>(level =>
        {
            var l = (level ?? "INFO").ToUpperInvariant();
            return l switch
            {
                "ERROR" => ThemeBrush("DestructiveColor", "#ef4444"),
                "WARN" => ThemeBrush("WarningColor", "#f59e0b"),
                "DEBUG" => ThemeBrush("MutedForegroundColor", "#6b7280"),
                _ => ThemeBrush("SuccessColor", "#10b981")
            };
        });

    public static readonly IValueConverter LogLevelToBackgroundBrush =
        new FuncValueConverter<string?, IBrush>(level =>
        {
            var l = (level ?? "INFO").ToUpperInvariant();
            return l switch
            {
                "ERROR" => new SolidColorBrush(Color.FromArgb(35, 239, 68, 68)),
                "WARN" => new SolidColorBrush(Color.FromArgb(35, 245, 158, 11)),
                "DEBUG" => new SolidColorBrush(Color.FromArgb(25, 107, 114, 128)),
                _ => new SolidColorBrush(Color.FromArgb(35, 16, 185, 129))
            };
        });

    public static readonly IValueConverter TimestampToTimeStr =
        new FuncValueConverter<string?, string>(ts =>
        {
            if (string.IsNullOrEmpty(ts)) return "";
            if (ts.Length > 8 && ts.Contains('T'))
            {
                var parts = ts.Split('T');
                if (parts.Length > 1 && parts[1].Length >= 8)
                    return "[" + parts[1].Substring(0, 8) + "]";
            }
            return "[" + ts + "]";
        });

    public static readonly IValueConverter RunningToTunnelText =
        new FuncValueConverter<bool, string>(running =>
            running
                ? (Prism.I18n.LocalizationManager.Instance["client_disconnect_tunnel"] ?? "断开隧道")
                : (Prism.I18n.LocalizationManager.Instance["client_connect"] ?? "连接"));

    public static readonly IValueConverter LocaleToShortLabel =
        new FuncValueConverter<string?, string>(locale =>
            string.Equals(locale, "zh-CN", StringComparison.OrdinalIgnoreCase)
                ? (Prism.I18n.LocalizationManager.Instance["language_chinese_short"] ?? "简中")
                : (Prism.I18n.LocalizationManager.Instance["language_english_short"] ?? "EN"));

    public static readonly IValueConverter StatusToLabel =
        new FuncValueConverter<bool, string>(running =>
            running
                ? (Prism.I18n.LocalizationManager.Instance["status_running"] ?? "RUNNING")
                : (Prism.I18n.LocalizationManager.Instance["status_idle"] ?? "IDLE"));

    public static readonly IValueConverter ActiveStatusToLabel =
        new FuncValueConverter<bool, string>(active =>
            active
                ? (Prism.I18n.LocalizationManager.Instance["status_active"] ?? "ACTIVE")
                : (Prism.I18n.LocalizationManager.Instance["status_disabled"] ?? "DISABLED"));

    public static readonly IValueConverter HealthToLabel =
        new FuncValueConverter<bool, string>(healthy =>
            healthy
                ? (Prism.I18n.LocalizationManager.Instance["status_healthy"] ?? "HEALTHY")
                : (Prism.I18n.LocalizationManager.Instance["status_unhealthy"] ?? "UNHEALTHY"));

    public static readonly IValueConverter BooleanToOpacity =
        new FuncValueConverter<bool, double>(enabled => enabled ? 1.0 : 0.5);

    public static readonly IValueConverter PrimaryStandbyLabel =
        new FuncValueConverter<bool, string>(primary =>
            primary
                ? (Prism.I18n.LocalizationManager.Instance["admin_primary"] ?? "Primary")
                : (Prism.I18n.LocalizationManager.Instance["admin_standby"] ?? "Standby"));

    public static readonly IValueConverter ActiveStatusToBrush =
        new FuncValueConverter<bool, IBrush>(active =>
            active ? ThemeBrush("SuccessColor", "#10b981") : ThemeBrush("MutedForegroundColor", "#6b7280"));

    private static IBrush ThemeBrush(string key, string fallbackHex)
    {
        if (Application.Current?.TryGetResource(key, null, out var value) == true && value is IBrush brush)
        {
            return brush;
        }
        return new SolidColorBrush(Color.Parse(fallbackHex));
    }

    public static readonly IValueConverter BooleanToString =
        new FuncValueConverter<bool, string, string>((val, param) =>
        {
            if (string.IsNullOrWhiteSpace(param)) return val ? "True" : "False";
            var parts = param.Split('|');
            return val ? parts[0] : (parts.Length > 1 ? parts[1] : "");
        });

    public static readonly IValueConverter CopiedOrCopyLabel =
        new FuncValueConverter<bool, string>(copied =>
            copied
                ? (Prism.I18n.LocalizationManager.Instance["common_copied"] ?? "Copied")
                : (Prism.I18n.LocalizationManager.Instance["common_copy"] ?? "Copy"));

    public static readonly IValueConverter CopiedOrShareLabel =
        new FuncValueConverter<bool, string>(copied =>
            copied
                ? (Prism.I18n.LocalizationManager.Instance["common_copied"] ?? "Copied")
                : (Prism.I18n.LocalizationManager.Instance["client_share_config"] ?? "Share"));

    public static readonly IValueConverter ExpirationDaysLabel =
        new FuncValueConverter<int, string>(days =>
            string.Equals(Prism.I18n.LocalizationManager.Instance.CurrentLocale, "zh-CN", StringComparison.OrdinalIgnoreCase)
                ? $"{days} 天"
                : (days == 1 ? "1 day" : $"{days} days"));

    public static readonly IValueConverter RoleLabel =
        new FuncValueConverter<string?, string>(r => r?.ToLowerInvariant() switch
        {
            "admin" => Prism.I18n.LocalizationManager.Instance["users_session_admin"] ?? "ADMIN",
            "member" => string.Equals(Prism.I18n.LocalizationManager.Instance.CurrentLocale, "zh-CN", StringComparison.OrdinalIgnoreCase) ? "成员" : "Member",
            "disabled" => Prism.I18n.LocalizationManager.Instance["status_disabled"] ?? "DISABLED",
            _ => r?.ToUpperInvariant() ?? ""
        });
}

public class LocaleOption
{
    public string Key { get; set; } = "";
    public string Value { get; set; } = "";

    public LocaleOption() { }
    public LocaleOption(string key, string value)
    {
        Key = key;
        Value = value;
    }
}
