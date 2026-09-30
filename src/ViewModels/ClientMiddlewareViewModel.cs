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

    private string _value = "";
    public string Value
    {
        get => _value;
        set => SetProperty(ref _value, value);
    }
}

public partial class ClientMiddlewareViewModel : ViewModelBase
{
    private readonly NativeClientService _client = NativeClientService.Instance;

    public ObservableCollection<MiddlewareItem> Middlewares { get; } = new();
    public ObservableCollection<MiddlewareFieldItem> ConfigFields { get; } = new();

    [ObservableProperty]
    private Prism.Native.MiddlewareItem? _selectedMiddleware;

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
        if (value?.Schema == null) return;

        foreach (var f in value.Schema.Fields)
        {
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
                Label = f.Label,
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
