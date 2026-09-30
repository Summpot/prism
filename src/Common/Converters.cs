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

    public static readonly IValueConverter NullOrEmptyToBoolean =
        new FuncValueConverter<string?, bool>(s => !string.IsNullOrWhiteSpace(s));

    public static readonly IValueConverter StatusToBrush =
        new FuncValueConverter<bool, IBrush>(running =>
            running ? new SolidColorBrush(Color.Parse("#10b981")) : new SolidColorBrush(Color.Parse("#6b7280")));

    public static readonly IValueConverter RouteEquals =
        new FuncValueConverter<string?, string, bool>((current, target) =>
            string.Equals(current, target, StringComparison.OrdinalIgnoreCase));

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

    public static readonly IValueConverter RunningToTunnelText =
        new FuncValueConverter<bool, string>(running =>
            running
                ? (Prism.I18n.LocalizationManager.Instance["client_disconnect_tunnel"] ?? "断开隧道")
                : (Prism.I18n.LocalizationManager.Instance["client_connect"] ?? "连接"));

    public static readonly IValueConverter StatusToLabel =
        new FuncValueConverter<bool, string>(running => running ? "RUNNING" : "IDLE");

    public static readonly IValueConverter ActiveStatusToLabel =
        new FuncValueConverter<bool, string>(active => active ? "ACTIVE" : "DISABLED");

    public static readonly IValueConverter ActiveStatusToBrush =
        new FuncValueConverter<bool, IBrush>(active =>
            active ? new SolidColorBrush(Color.Parse("#10b981")) : new SolidColorBrush(Color.Parse("#6b7280")));
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
