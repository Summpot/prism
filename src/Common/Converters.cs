using System;
using System.Collections.Generic;
using System.Globalization;
using Avalonia;
using Avalonia.Controls.Primitives;
using Avalonia.Data.Converters;
using Avalonia.Media;

namespace Prism.Common;

public static class Converters
{
    public static readonly IValueConverter BoolToTextWrapping =
        new FuncValueConverter<bool, TextWrapping>(wrap =>
            wrap ? TextWrapping.Wrap : TextWrapping.NoWrap);

    public static readonly IValueConverter BoolToHorizontalScrollBarVisibility =
        new FuncValueConverter<bool, ScrollBarVisibility>(wrap =>
            wrap ? ScrollBarVisibility.Disabled : ScrollBarVisibility.Auto);

    public static readonly IValueConverter BooleanNegation =
        new FuncValueConverter<bool, bool>(b => !b);

    // ShadUI demo gutter. The sidebar scrollbar is overlaid, so the right inset keeps it off the items.
    public static readonly IValueConverter SidebarPadding =
        new FuncValueConverter<bool, Thickness>(expanded =>
            expanded ? new Thickness(16, 8) : new Thickness(8));

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
            Prism.I18n.Messages.MiddlewareSettingsCount(c));

    public static readonly IValueConverter AvatarInitials =
        new FuncValueConverter<string?, string>(name =>
        {
            if (string.IsNullOrWhiteSpace(name))
            {
                return "?";
            }

            var trimmed = name.Trim();
            var parts = trimmed.Split(' ', StringSplitOptions.RemoveEmptyEntries);
            if (parts.Length >= 2 && parts[0].Length > 0 && parts[1].Length > 0)
            {
                return string.Concat(char.ToUpperInvariant(parts[0][0]), char.ToUpperInvariant(parts[1][0]));
            }

            return trimmed.Length <= 2
                ? trimmed.ToUpperInvariant()
                : trimmed[..2].ToUpperInvariant();
        });

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
            "admin" => Prism.I18n.Messages.UsersTokenTypeAdmin(),
            "connector" => Prism.I18n.Messages.UsersTokenTypeConnector(),
            "client" => Prism.I18n.Messages.UsersTokenTypeClient(),
            _ => value ?? ""
        });

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
                "ERROR" => ThemeBrushAlpha("DestructiveColor", 0x23, "#ef4444"),
                "WARN" => ThemeBrushAlpha("WarningColor", 0x23, "#f59e0b"),
                "DEBUG" => ThemeBrushAlpha("MutedColor", 0x19, "#6b7280"),
                _ => ThemeBrushAlpha("SuccessColor", 0x23, "#10b981")
            };
        });

    public static readonly IMultiValueConverter MiddlewareDefaultLabel =
        new FuncMultiValueConverter<object?, string?>((IReadOnlyList<object?> values) =>
        {
            var value = values.Count > 0 ? values[0]?.ToString() ?? "" : "";
            var template = values.Count > 1 ? values[1]?.ToString() : null;
            if (string.IsNullOrEmpty(template) || template == "middleware_default")
            {
                return Prism.I18n.Messages.MiddlewareDefault(value);
            }

            return template.Replace("{value}", value);
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
        if (TryGetThemeColor(key, out var color))
        {
            return new SolidColorBrush(color);
        }

        if (Application.Current?.TryGetResource(key, null, out var value) == true && value is IBrush brush)
        {
            return brush;
        }

        return new SolidColorBrush(Color.Parse(fallbackHex));
    }

    private static IBrush ThemeBrushAlpha(string key, byte alpha, string fallbackHex)
    {
        var color = TryGetThemeColor(key, out var theme)
            ? theme
            : Color.Parse(fallbackHex);
        return new SolidColorBrush(Color.FromArgb(alpha, color.R, color.G, color.B));
    }

    private static bool TryGetThemeColor(string key, out Color color)
    {
        color = default;
        if (Application.Current?.TryGetResource(key, null, out var value) != true || value == null)
        {
            return false;
        }

        switch (value)
        {
            case Color direct:
                color = direct;
                return true;
            case ISolidColorBrush solid:
                color = solid.Color;
                return true;
            default:
                return false;
        }
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

    public static readonly IValueConverter UpdateChannelLabel =
        new FuncValueConverter<string?, string>(ch => ch?.ToLowerInvariant() switch
        {
            "dev" => Prism.I18n.LocalizationManager.Instance["client_update_channel_dev"] ?? "Dev (Preview)",
            _ => Prism.I18n.LocalizationManager.Instance["client_update_channel_release"] ?? "Release (Stable)"
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
