using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Linq;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Prism.Native;
using Prism.Services;

namespace Prism.ViewModels;

public class MiddlewareFieldItem : ObservableObject
{
    public string Key { get; set; } = "";
    public string Label { get; set; } = "";
    public string Description { get; set; } = "";
    public string FieldType { get; set; } = "string";

    public bool IsBool => FieldType.Equals("bool", StringComparison.OrdinalIgnoreCase) || FieldType.Equals("boolean", StringComparison.OrdinalIgnoreCase);
    public bool IsNumber => FieldType.Equals("number", StringComparison.OrdinalIgnoreCase) || FieldType.Equals("int", StringComparison.OrdinalIgnoreCase) || FieldType.Equals("integer", StringComparison.OrdinalIgnoreCase) || FieldType.Equals("float", StringComparison.OrdinalIgnoreCase);
    public bool IsString => !IsBool && !IsNumber;

    private string _value = "";
    public string Value
    {
        get => _value;
        set
        {
            if (SetProperty(ref _value, value))
            {
                OnPropertyChanged(nameof(BoolValue));
                OnPropertyChanged(nameof(NumberValue));
            }
        }
    }

    public bool BoolValue
    {
        get => bool.TryParse(_value, out var b) && b;
        set => Value = value.ToString().ToLowerInvariant();
    }

    public decimal NumberValue
    {
        get => decimal.TryParse(_value, out var d) ? d : 0;
        set => Value = value.ToString();
    }
}

public partial class ClientMiddlewareViewModel : ViewModelBase
{
    private readonly NativeClientService _client = NativeClientService.Instance;

    public ObservableCollection<Prism.Native.MiddlewareItem> Middlewares { get; } = new();
    public ObservableCollection<MiddlewareFieldItem> ConfigFields { get; } = new();

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

    public void LoadData()
    {
        Middlewares.Clear();
        try
        {
            var list = _client.ListMiddlewares();
            foreach (var item in list)
            {
                Middlewares.Add(item);
            }
            SelectedMiddleware = Middlewares.FirstOrDefault();
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

            string currentVal = "";
            if (value.EffectiveConfig.TryGetValue(f.Key, out var v))
            {
                currentVal = v;
            }
            else
            {
                currentVal = f.DefaultValue;
            }

            ConfigFields.Add(new MiddlewareFieldItem
            {
                Key = f.Key,
                Label = string.IsNullOrWhiteSpace(f.Label) ? f.Key : f.Label,
                Description = f.Description,
                FieldType = f.FieldType,
                Value = currentVal
            });
        }
    }

    [RelayCommand]
    public void SaveConfig()
    {
        if (SelectedMiddleware == null) return;

        try
        {
            var dict = ConfigFields.ToDictionary(f => f.Key, f => f.Value);
            dict["enabled"] = IsMiddlewareEnabled ? "true" : "false";

            var updated = _client.UpdateMiddlewareConfig(SelectedMiddleware.Name, dict);
            StatusMessage = $"Config for {updated.Name} saved successfully.";
            LoadData();
        }
        catch (Exception ex)
        {
            StatusMessage = ex.Message;
        }
    }

    [RelayCommand]
    public void ResetConfig()
    {
        if (SelectedMiddleware == null) return;

        try
        {
            var reset = _client.ResetMiddlewareConfig(SelectedMiddleware.Name);
            StatusMessage = $"Config for {reset.Name} reset to default.";
            LoadData();
        }
        catch (Exception ex)
        {
            StatusMessage = ex.Message;
        }
    }
}
