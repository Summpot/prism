using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Linq;
using System.Threading.Tasks;
using Avalonia.Input.Platform;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Prism.I18n;
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

    private bool _autoConnect = false;
    public bool AutoConnect
    {
        get => _autoConnect;
        set => SetProperty(ref _autoConnect, value);
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

    public EditableProfile Clone()
    {
        return new EditableProfile
        {
            Id = Id,
            Name = Name,
            ServerAddr = ServerAddr,
            Transport = Transport,
            ListenAddr = ListenAddr,
            FakeLanBroadcast = FakeLanBroadcast,
            AutoConnect = AutoConnect,
            AuthToken = AuthToken,
            IsActive = IsActive
        };
    }

    public void CopyFrom(EditableProfile other)
    {
        Id = other.Id;
        Name = other.Name;
        ServerAddr = other.ServerAddr;
        Transport = other.Transport;
        ListenAddr = other.ListenAddr;
        FakeLanBroadcast = other.FakeLanBroadcast;
        AutoConnect = other.AutoConnect;
        AuthToken = other.AuthToken;
        IsActive = other.IsActive;
    }
}

public partial class ClientProfilesViewModel : ViewModelBase, INavigationAware
{
    private readonly NativeClientService _client = NativeClientService.Instance;

    public List<string> AvailableTransports { get; } = new()
    {
        "auto",
        "webtransport",
        "tcp",
        "kcp",
        "quic",
        "websocket"
    };

    public ObservableCollection<EditableProfile> Profiles { get; } = new();

    [ObservableProperty]
    private EditableProfile? _selectedProfile;

    [ObservableProperty]
    private EditableProfile? _draftProfile;

    [ObservableProperty]
    private bool _isDirty;

    [ObservableProperty]
    private bool _isCopied;

    [ObservableProperty]
    private string _profilesCountText = "";

    [ObservableProperty]
    private string _editProfileTitle = "";

    private bool _suppressSelectionChange;
    private EditableProfile? _previousProfile;

    partial void OnSelectedProfileChanging(EditableProfile? oldValue, EditableProfile? newValue)
    {
        if (_suppressSelectionChange) return;
        _previousProfile = oldValue;
    }

    partial void OnSelectedProfileChanged(EditableProfile? value)
    {
        if (_suppressSelectionChange) return;

        if (IsDirty && _previousProfile != null && value != _previousProfile)
        {
            var prev = _previousProfile;
            var prevDraft = DraftProfile;
            Avalonia.Threading.Dispatcher.UIThread.Post(async () =>
            {
                bool discard = await AppServices.ConfirmAsync(
                    I18nText.T("nodecfg_unsaved", "未保存的更改"),
                    I18nText.T("client_discard_prompt", "当前配置存在未保存的修改，确定要放弃修改并切换吗？"),
                    I18nText.T("common_discard", "放弃"));
                if (!discard)
                {
                    _suppressSelectionChange = true;
                    SelectedProfile = prev;
                    DraftProfile = prevDraft;
                    IsDirty = true;
                    _suppressSelectionChange = false;
                    UpdateComputed();
                }
            });
        }

        PingMs = null;
        if (value != null)
        {
            var draft = value.Clone();
            draft.PropertyChanged += (_, _) =>
            {
                IsDirty = true;
                UpdateComputed();
            };
            DraftProfile = draft;
            IsDirty = false;
        }
        else
        {
            DraftProfile = null;
            IsDirty = false;
        }
        UpdateComputed();
    }

    protected override void OnLocaleChanged()
    {
        base.OnLocaleChanged();
        UpdateComputed();
    }

    private void UpdateComputed()
    {
        string tpl = I18nText.T("client_profile_count", "{count} 个配置");
        ProfilesCountText = tpl.Replace("{count}", Profiles.Count.ToString());

        string editTpl = I18nText.T("client_edit_profile", "编辑配置：{name}");
        EditProfileTitle = editTpl.Replace("{name}", DraftProfile?.Name ?? "");
    }

    [RelayCommand]
    public void RevertDraft()
    {
        if (SelectedProfile != null)
        {
            var draft = SelectedProfile.Clone();
            draft.PropertyChanged += (_, _) =>
            {
                IsDirty = true;
                UpdateComputed();
            };
            DraftProfile = draft;
            IsDirty = false;
            AppServices.ShowInfo(I18nText.T("client_reverted_toast", "已放弃对配置的修改"), I18nText.T("common_revert", "已还原"));
        }
    }

    [ObservableProperty]
    private string? _statusMessage;

    [ObservableProperty]
    private string _importInputText = "";

    [ObservableProperty]
    private bool _showImportBox;

    [ObservableProperty]
    private int? _pingMs;

    [ObservableProperty]
    private bool _isPinging;

    public ClientProfilesViewModel()
    {
        LoadProfiles();
    }

    public void OnNavigatedTo()
    {
        LoadProfiles();
    }

    public void OnNavigatedFrom()
    {
    }

    private void LoadProfiles()
    {
        var prevSelectedId = SelectedProfile?.Id;
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
                    IsActive = p.Id == activeId
                });
            }

            SelectedProfile = (prevSelectedId != null ? Profiles.FirstOrDefault(p => p.Id == prevSelectedId) : null)
                ?? Profiles.FirstOrDefault(p => p.IsActive)
                ?? Profiles.FirstOrDefault();
            UpdateComputed();
        }
        catch (Exception ex)
        {
            StatusMessage = ex.Message;
        }
    }

    [RelayCommand]
    public void ToggleImportBox()
    {
        ShowImportBox = !ShowImportBox;
    }

    [RelayCommand]
    public void ImportLink()
    {
        if (string.IsNullOrWhiteSpace(ImportInputText)) return;

        var parsed = PrismLinkService.Parse(ImportInputText);
        if (parsed == null || string.IsNullOrWhiteSpace(parsed.ServerAddr))
        {
            StatusMessage = "Invalid prism:// link or address.";
            return;
        }

        var newProf = new EditableProfile
        {
            Id = $"profile-{DateTimeOffset.UtcNow.ToUnixTimeMilliseconds()}",
            Name = string.IsNullOrWhiteSpace(parsed.Name) ? parsed.ServerAddr : parsed.Name,
            ServerAddr = parsed.ServerAddr,
            Transport = parsed.Transport,
            AuthToken = parsed.AuthToken ?? "",
            ListenAddr = parsed.ListenAddr,
            FakeLanBroadcast = parsed.FakeLanBroadcast
        };

        Profiles.Add(newProf);
        SelectedProfile = newProf;
        ImportInputText = "";
        ShowImportBox = false;
        SaveSelectedProfile();
        StatusMessage = $"Imported profile '{newProf.Name}'.";
    }

    [RelayCommand]
    public async Task ShareLinkAsync()
    {
        if (SelectedProfile == null) return;
        var link = PrismLinkService.Encode(
            SelectedProfile.Name,
            SelectedProfile.ServerAddr,
            SelectedProfile.Transport,
            SelectedProfile.ListenAddr,
            SelectedProfile.FakeLanBroadcast,
            SelectedProfile.AuthToken
        );

        if (Avalonia.Application.Current?.ApplicationLifetime is Avalonia.Controls.ApplicationLifetimes.IClassicDesktopStyleApplicationLifetime desktop && desktop.MainWindow?.Clipboard != null)
        {
            await desktop.MainWindow.Clipboard.SetTextAsync(link);
            StatusMessage = "Prism link copied to clipboard!";
            IsCopied = true;
            await Task.Delay(2000);
            IsCopied = false;
        }
    }

    [RelayCommand]
    public async Task PingServerAsync()
    {
        if (SelectedProfile == null || string.IsNullOrWhiteSpace(SelectedProfile.ServerAddr)) return;
        IsPinging = true;
        PingMs = null;
        try
        {
            var sw = System.Diagnostics.Stopwatch.StartNew();
            string addr = SelectedProfile.ServerAddr.Trim();
            int schemeIdx = addr.IndexOf("://", StringComparison.Ordinal);
            if (schemeIdx >= 0)
            {
                addr = addr[(schemeIdx + 3)..];
            }
            string host = addr;
            int port = 7000;
            if (addr.StartsWith('['))
            {
                int close = addr.IndexOf(']');
                if (close > 0)
                {
                    host = addr.Substring(1, close - 1);
                    if (close + 1 < addr.Length && addr[close + 1] == ':')
                    {
                        int.TryParse(addr[(close + 2)..], out port);
                    }
                }
            }
            else if (addr.Contains(':'))
            {
                var parts = addr.Split(':');
                host = parts[0];
                int.TryParse(parts[^1], out port);
            }

            using var client = new System.Net.Sockets.TcpClient();
            using var cts = new System.Threading.CancellationTokenSource(TimeSpan.FromSeconds(3));
            await client.ConnectAsync(host, port, cts.Token);
            sw.Stop();
            PingMs = (int)sw.ElapsedMilliseconds;
            StatusMessage = $"Ping to {host}:{port} succeeded in {PingMs}ms.";
        }
        catch (Exception ex)
        {
            PingMs = -1;
            StatusMessage = $"Ping failed: {ex.Message}";
        }
        finally
        {
            IsPinging = false;
        }
    }

    [RelayCommand]
    public void NewProfile()
    {
        var p = new EditableProfile
        {
            Id = $"profile-{DateTimeOffset.UtcNow.ToUnixTimeMilliseconds()}",
            Name = "New Profile",
            ServerAddr = "relay.example.com",
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
            if (DraftProfile != null)
            {
                SelectedProfile.CopyFrom(DraftProfile);
                IsDirty = false;
            }

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

            if (SelectedProfile.IsActive)
            {
                var patch = new ClientConfigPatch(
                    ProfileName: SelectedProfile.Name,
                    ServerAddr: SelectedProfile.ServerAddr,
                    Transport: SelectedProfile.Transport,
                    AuthToken: SelectedProfile.AuthToken,
                    ListenAddr: SelectedProfile.ListenAddr,
                    FakeLanBroadcast: SelectedProfile.FakeLanBroadcast,
                    AutoConnectPanel: null,
                    AutoConnect: SelectedProfile.AutoConnect,
                    ManagementUrl: null,
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
            }
            else
            {
                var patch = new ClientConfigPatch(
                    ProfileName: null,
                    ServerAddr: null,
                    Transport: null,
                    AuthToken: null,
                    ListenAddr: null,
                    FakeLanBroadcast: null,
                    AutoConnectPanel: null,
                    AutoConnect: null,
                    ManagementUrl: null,
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
                    ActiveProfileId: null,
                    ActiveConfig: patch
                ));
            }

            StatusMessage = I18n.I18nText.T("common_save");
            AppServices.ShowSuccess(SelectedProfile.Name);
        }
        catch (Exception ex)
        {
            StatusMessage = ex.Message;
            AppServices.ShowError(ex.Message, I18n.I18nText.T("common_save_failed"));
        }
    }

    [RelayCommand]
    public async Task DeleteProfileAsync(EditableProfile? profile)
    {
        var target = profile ?? SelectedProfile;
        if (target == null) return;
        if (!await AppServices.ConfirmAsync(
                I18n.I18nText.T("common_confirm"),
                I18n.I18nText.T("confirm_delete_profile"),
                I18n.I18nText.T("common_delete"),
                destructive: true))
        {
            return;
        }
        bool wasActive = target.IsActive;

        Profiles.Remove(target);
        if (SelectedProfile == target)
        {
            SelectedProfile = Profiles.FirstOrDefault();
        }

        if (wasActive && SelectedProfile != null)
        {
            SetActiveProfile();
        }
        else
        {
            SaveSelectedProfile();
        }
    }

    [RelayCommand]
    public async Task DeleteSelectedProfileAsync()
    {
        await DeleteProfileAsync(SelectedProfile);
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
                AutoConnectPanel: null,
                AutoConnect: SelectedProfile.AutoConnect,
                ManagementUrl: null,
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
