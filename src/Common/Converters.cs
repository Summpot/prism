using System;
using System.Globalization;
using Avalonia.Data.Converters;
using Avalonia.Media;

namespace Prism.Common;

public static class Converters
{
    public static readonly IValueConverter BooleanNegation =
        new FuncValueConverter<bool, bool>(b => !b);

    public static readonly IValueConverter EqualsZero =
        new FuncValueConverter<int, bool>(c => c == 0);

    public static readonly IValueConverter NullToBoolean =
        new FuncValueConverter<object?, bool>(o => o != null);

    public static readonly IValueConverter NotNullToBoolean = NullToBoolean;

    public static readonly IValueConverter NullToTrue =
        new FuncValueConverter<object?, bool>(o => o == null);

    public static readonly IValueConverter NullOrEmptyToBoolean =
        new FuncValueConverter<string?, bool>(s => !string.IsNullOrWhiteSpace(s));

    public static readonly IValueConverter BoolToCopyOrCheckIcon =
        new FuncValueConverter<bool, Geometry>(copied => copied ? AppIcons.Check : AppIcons.Copy);

    public static readonly IValueConverter BoolToCopyOrCheckBrush =
        new FuncValueConverter<bool, IBrush>(copied =>
            copied ? new SolidColorBrush(Color.Parse("#10b981")) : new SolidColorBrush(Color.Parse("#9ca3af")));

    public static readonly IValueConverter StatusToBrush =
        new FuncValueConverter<bool, IBrush>(running =>
            running ? new SolidColorBrush(Color.Parse("#10b981")) : new SolidColorBrush(Color.Parse("#6b7280")));

    public static readonly IValueConverter RouteEquals =
        new FuncValueConverter<string?, string, bool>((current, target) =>
            string.Equals(current, target, StringComparison.OrdinalIgnoreCase));

    public static readonly IValueConverter IntEquals =
        new FuncValueConverter<int, string, bool>((val, param) =>
            int.TryParse(param, out var target) && val == target);

    public static readonly IValueConverter LogLevelToBrush =
        new FuncValueConverter<string?, IBrush>(level =>
        {
            var l = (level ?? "INFO").ToUpperInvariant();
            return l switch
            {
                "ERROR" => new SolidColorBrush(Color.Parse("#ef4444")),
                "WARN" => new SolidColorBrush(Color.Parse("#f59e0b")),
                "DEBUG" => new SolidColorBrush(Color.Parse("#6b7280")),
                _ => new SolidColorBrush(Color.Parse("#10b981"))
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
        new FuncValueConverter<bool, string>(running => running ? "RUNNING" : "IDLE");

    public static readonly IValueConverter ActiveStatusToLabel =
        new FuncValueConverter<bool, string>(active => active ? "ACTIVE" : "DISABLED");

    public static readonly IValueConverter ActiveStatusToBrush =
        new FuncValueConverter<bool, IBrush>(active =>
            active ? new SolidColorBrush(Color.Parse("#10b981")) : new SolidColorBrush(Color.Parse("#6b7280")));

    public static readonly IValueConverter BooleanToString =
        new FuncValueConverter<bool, string, string>((val, param) =>
        {
            if (string.IsNullOrWhiteSpace(param)) return val ? "True" : "False";
            var parts = param.Split('|');
            return val ? parts[0] : (parts.Length > 1 ? parts[1] : "");
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
