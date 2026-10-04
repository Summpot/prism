using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Linq;
using System.Threading.Tasks;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Prism.I18n;
using Prism.Native;
using Prism.Services;

namespace Prism.ViewModels;

public abstract class MiddlewareFieldViewModel : ObservableObject
{
    public string Key { get; set; } = "";
    public string Label { get; set; } = "";
    public string Description { get; set; } = "";
    public string DefaultValue { get; set; } = "";

    public bool IsModified => !string.Equals(GetStringValue().Trim(), DefaultValue.Trim(), StringComparison.OrdinalIgnoreCase);

    public abstract string GetStringValue();
    public abstract void SetStringValue(string value);

    public static MiddlewareFieldViewModel Create(string key, string label, string description, string fieldType, string initialValue, string defaultValue)
    {
        MiddlewareFieldViewModel vm = fieldType.ToLowerInvariant() switch
        {
            "bool" or "boolean" => new BoolMiddlewareFieldViewModel(),
            "number" or "int" or "integer" or "float" or "u8" or "u16" or "u32" or "i32" or "i64" or "f32" or "f64" => new NumberMiddlewareFieldViewModel(),
            "list_string" or "string[]" or "list" => new ListStringMiddlewareFieldViewModel(),
            _ => new StringMiddlewareFieldViewModel()
        };
        vm.Key = key;
        vm.Label = label;
        vm.Description = description;
        vm.DefaultValue = defaultValue;
        vm.SetStringValue(initialValue);
        return vm;
    }
}

public partial class BoolMiddlewareFieldViewModel : MiddlewareFieldViewModel
{
    [ObservableProperty]
    private bool _value;

    partial void OnValueChanged(bool value) => OnPropertyChanged(nameof(IsModified));

    public override string GetStringValue() => Value ? "true" : "false";
    public override void SetStringValue(string value) => Value = bool.TryParse(value, out var b) && b;
}

public partial class NumberMiddlewareFieldViewModel : MiddlewareFieldViewModel
{
    [ObservableProperty]
    private decimal _value;

    partial void OnValueChanged(decimal value) => OnPropertyChanged(nameof(IsModified));

    public override string GetStringValue() => Value.ToString(System.Globalization.CultureInfo.InvariantCulture);
    public override void SetStringValue(string value) => Value = decimal.TryParse(value, System.Globalization.NumberStyles.Any, System.Globalization.CultureInfo.InvariantCulture, out var d) ? d : 0;
}

public partial class StringMiddlewareFieldViewModel : MiddlewareFieldViewModel
{
    [ObservableProperty]
    private string _value = "";

    partial void OnValueChanged(string value) => OnPropertyChanged(nameof(IsModified));

    public override string GetStringValue() => Value;
    public override void SetStringValue(string value) => Value = value ?? "";
}

public partial class ListStringMiddlewareFieldViewModel : MiddlewareFieldViewModel
{
    [ObservableProperty]
    private string _value = "";

    partial void OnValueChanged(string value) => OnPropertyChanged(nameof(IsModified));

    public override string GetStringValue() => Value;
    public override void SetStringValue(string value) => Value = value ?? "";
}

public partial class ClientMiddlewareViewModel : ViewModelBase, INavigationAware
{
    private readonly NativeClientService _client = NativeClientService.Instance;

    public ObservableCollection<Prism.Native.MiddlewareItem> Middlewares { get; } = new();
    public ObservableCollection<MiddlewareFieldViewModel> ConfigFields { get; } = new();

    [ObservableProperty]
    private Prism.Native.MiddlewareItem? _selectedMiddleware;

    [ObservableProperty]
    private bool _isMiddlewareEnabled = true;

    [ObservableProperty]
    private string? _statusMessage;

    public ClientMiddlewareViewModel()
    {
        LoadData();
    }

    public void OnNavigatedTo()
    {
        LoadData(SelectedMiddleware?.Name);
    }

    public void OnNavigatedFrom()
    {
    }

    public void LoadData(string? preserveName = null)
    {
        Middlewares.Clear();
        try
        {
            var list = _client.ListMiddlewares();
            foreach (var item in list)
            {
                Middlewares.Add(item);
            }
            SelectedMiddleware = (!string.IsNullOrWhiteSpace(preserveName)
                ? Middlewares.FirstOrDefault(m => string.Equals(m.Name, preserveName, StringComparison.OrdinalIgnoreCase))
                : null) ?? Middlewares.FirstOrDefault();
        }
        catch (Exception ex)
        {
            StatusMessage = ex.Message;
        }
    }

    partial void OnSelectedMiddlewareChanged(Prism.Native.MiddlewareItem? value)
    {
        ConfigFields.Clear();
        if (value == null) return;

        if (value.EffectiveConfig.TryGetValue("enabled", out var en))
        {
            IsMiddlewareEnabled = !string.Equals(en, "false", StringComparison.OrdinalIgnoreCase);
        }
        else
        {
            IsMiddlewareEnabled = true;
        }

        if (value.Schema == null) return;

        foreach (var f in value.Schema.Fields)
        {
            if (f.Key.Equals("enabled", StringComparison.OrdinalIgnoreCase)) continue;

            string currentVal = value.EffectiveConfig.TryGetValue(f.Key, out var v) ? v : f.DefaultValue;

            ConfigFields.Add(MiddlewareFieldViewModel.Create(
                key: f.Key,
                label: string.IsNullOrWhiteSpace(f.Label) ? f.Key : f.Label,
                description: f.Description,
                fieldType: f.FieldType,
                initialValue: currentVal,
                defaultValue: f.DefaultValue
            ));
        }
    }

    [RelayCommand]
    public void SaveConfig()
    {
        if (SelectedMiddleware == null) return;

        try
        {
            var dict = ConfigFields.ToDictionary(f => f.Key, f => f.GetStringValue());
            dict["enabled"] = IsMiddlewareEnabled ? "true" : "false";

            string currentName = SelectedMiddleware.Name;
            var updated = _client.UpdateMiddlewareConfig(currentName, dict);
            StatusMessage = Messages.MiddlewareApplied(currentName);
            LoadData(currentName);
            AppServices.ShowSuccess(StatusMessage);
        }
        catch (Exception ex)
        {
            StatusMessage = ex.Message;
        }
    }

    [RelayCommand]
    public async Task ResetConfigAsync()
    {
        if (SelectedMiddleware == null) return;
        if (!await AppServices.ConfirmAsync(
                Messages.CommonConfirm(),
                Messages.ConfirmResetMiddleware(),
                Messages.CommonReset()))
        {
            return;
        }

        try
        {
            string currentName = SelectedMiddleware.Name;
            var reset = _client.ResetMiddlewareConfig(currentName);
            StatusMessage = Messages.MiddlewareResetDone(currentName);
            LoadData(currentName);
            AppServices.ShowSuccess(StatusMessage);
        }
        catch (Exception ex)
        {
            StatusMessage = ex.Message;
            AppServices.ShowError(ex.Message);
        }
    }
}
