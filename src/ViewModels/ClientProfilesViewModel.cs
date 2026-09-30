using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Linq;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Prism.Native;
using Prism.Services;

namespace Prism.ViewModels;

public class EditableProfile : ObservableObject
{
    public string Id { get; set; } = "";

    private string _name = "New Profile";
    public string Name
    {
        get => _name;
        set => SetProperty(ref _name, value);
    }

    private string _serverAddr = "relay.example.com:443";
    public string ServerAddr
    {
        get => _serverAddr;
        set => SetProperty(ref _serverAddr, value);
    }

    private string _transport = "auto";
    public string Transport
    {
        get => _transport;
        set => SetProperty(ref _transport, value);
    }

    private string _listenAddr = "127.0.0.1:25565";
    public string ListenAddr
    {
        get => _listenAddr;
        set => SetProperty(ref _listenAddr, value);
    }

    private bool _fakeLanBroadcast = true;
    public bool FakeLanBroadcast
    {
        get => _fakeLanBroadcast;
        set => SetProperty(ref _fakeLanBroadcast, value);
    }

    private bool _autoConnectPanel = false;
    public bool AutoConnectPanel
    {
        get => _autoConnectPanel;
        set => SetProperty(ref _autoConnectPanel, value);
    }

    private bool _autoConnect = false;
    public bool AutoConnect
    {
        get => _autoConnect;
        set => SetProperty(ref _autoConnect, value);
    }

    private string _managementUrl = "";
    public string ManagementUrl
    {
        get => _managementUrl;
        set => SetProperty(ref _managementUrl, value);
    }

    private string _authToken = "";
    public string AuthToken
    {
        get => _authToken;
        set => SetProperty(ref _authToken, value);
    }

    private bool _isActive;
    public bool IsActive
    {
        get => _isActive;
        set => SetProperty(ref _isActive, value);
    }
}

public partial class ClientProfilesViewModel : ViewModelBase
{
    private readonly NativeClientService _client = NativeClientService.Instance;

    public List<string> AvailableTransports { get; } = new()
    {
        "auto",
        "tcp",
        "kcp",
        "quic",
        "ws"
    };

    public ObservableCollection<EditableProfile> Profiles { get; } = new();

    [ObservableProperty]
    private EditableProfile? _selectedProfile;

    [ObservableProperty]
    private string? _statusMessage;

    public ClientProfilesViewModel()
    {
        LoadProfiles();
    }

    private void LoadProfiles()
    {
        Profiles.Clear();
        try
        {
            var rawProfiles = _client.GetProfiles();
            var cfg = _client.GetConfig();
            string? activeId = cfg.ActiveProfileId;

            foreach (var p in rawProfiles)
            {
                Profiles.Add(new EditableProfile
                {
                    Id = p.Id,
                    Name = p.Name,
                    ServerAddr = p.ServerAddr,
                    Transport = p.Transport,
                    AuthToken = p.AuthToken,
                    ListenAddr = p.ListenAddr,
                    FakeLanBroadcast = p.FakeLanBroadcast,
                    AutoConnect = cfg.ActiveConfig.AutoConnect,
                    AutoConnectPanel = cfg.ActiveConfig.AutoConnectPanel,
                    ManagementUrl = cfg.ActiveConfig.ManagementUrl ?? "",
                    IsActive = p.Id == activeId
                });
            }

            SelectedProfile = Profiles.FirstOrDefault(p => p.IsActive) ?? Profiles.FirstOrDefault();
        }
        catch (Exception ex)
        {
            StatusMessage = ex.Message;
        }
    }

    [RelayCommand]
    public void NewProfile()
    {
        var p = new EditableProfile
        {
            Id = $"profile-{DateTimeOffset.UtcNow.ToUnixTimeMilliseconds()}",
            Name = "New Profile",
            ServerAddr = "127.0.0.1:443",
            Transport = "auto",
            ListenAddr = "127.0.0.1:25565",
            FakeLanBroadcast = true
        };
        Profiles.Add(p);
        SelectedProfile = p;
    }

    [RelayCommand]
    public void SaveSelectedProfile()
    {
        if (SelectedProfile == null) return;
        try
        {
            var list = Profiles.Select(p => new ClientProfile(
                Id: p.Id,
                Name: p.Name,
                ServerAddr: p.ServerAddr,
                Transport: p.Transport,
                AuthToken: p.AuthToken ?? "",
                ListenAddr: p.ListenAddr,
                FakeLanBroadcast: p.FakeLanBroadcast
            )).ToList();

            _client.SaveProfiles(list);
            StatusMessage = "Profile saved successfully.";
        }
        catch (Exception ex)
        {
            StatusMessage = ex.Message;
        }
    }

    [RelayCommand]
    public void DeleteProfile(EditableProfile? profile)
    {
        var target = profile ?? SelectedProfile;
        if (target == null) return;
        Profiles.Remove(target);
        if (SelectedProfile == target)
        {
            SelectedProfile = Profiles.FirstOrDefault();
        }
        SaveSelectedProfile();
    }

    [RelayCommand]
    public void DeleteSelectedProfile()
    {
        DeleteProfile(SelectedProfile);
    }

    [RelayCommand]
    public void SetActiveProfile()
    {
        if (SelectedProfile == null) return;
        foreach (var p in Profiles)
        {
            p.IsActive = p.Id == SelectedProfile.Id;
        }

        try
        {
            var patch = new ClientConfigPatch(
                ProfileName: SelectedProfile.Name,
                ServerAddr: SelectedProfile.ServerAddr,
                Transport: SelectedProfile.Transport,
                AuthToken: SelectedProfile.AuthToken,
                ListenAddr: SelectedProfile.ListenAddr,
                FakeLanBroadcast: SelectedProfile.FakeLanBroadcast,
                AutoConnectPanel: SelectedProfile.AutoConnectPanel,
                AutoConnect: SelectedProfile.AutoConnect,
                ManagementUrl: string.IsNullOrWhiteSpace(SelectedProfile.ManagementUrl) ? null : SelectedProfile.ManagementUrl,
                TokenId: null,
                TokenType: null,
                UserId: null,
                Username: null,
                ExpiresAt: null,
                AutoCheckUpdate: null,
                UpdateChannel: null,
                Autostart: null,
                SilentAutostart: null,
                OptimizerEnabled: null,
                OptimizerZstdLevel: null,
                OptimizerAdaptiveFlush: null,
                OptimizerFlushIntervalMs: null,
                OptimizerBufferThreshold: null
            );

            _client.SaveConfig(new SaveConfigRequest(
                ActiveProfileId: SelectedProfile.Id,
                ActiveConfig: patch
            ));
            StatusMessage = $"'{SelectedProfile.Name}' set as active profile.";
        }
        catch (Exception ex)
        {
            StatusMessage = ex.Message;
        }
    }
}
