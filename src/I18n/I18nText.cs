namespace Prism.I18n;

public static class I18nText
{
    public static string T(string key, string? fallback = null)
    {
        try
        {
            var value = LocalizationManager.Instance[key];
            if (string.IsNullOrEmpty(value) || value == key)
            {
                return fallback ?? key;
            }
            return value;
        }
        catch
        {
            return fallback ?? key;
        }
    }

    public static string Format(string key, params (string Name, object? Value)[] args)
    {
        var template = T(key);
        foreach (var (name, value) in args)
        {
            template = template.Replace("{" + name + "}", value?.ToString() ?? "");
        }
        return template;
    }
}
