using CommunityToolkit.Mvvm.ComponentModel;
using Prism.I18n;

namespace Prism.ViewModels;

public interface INavigationAware
{
    void OnNavigatedTo();
    void OnNavigatedFrom() { }
}

public abstract class ViewModelBase : ObservableObject
{
    protected ViewModelBase()
    {
        Messages.CurrentLocaleChanged += () =>
        {
            Avalonia.Threading.Dispatcher.UIThread.Post(OnLocaleChanged);
        };
    }

    protected virtual void OnLocaleChanged()
    {
    }
}
