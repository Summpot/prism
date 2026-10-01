using CommunityToolkit.Mvvm.ComponentModel;

namespace Prism.ViewModels;

public interface INavigationAware
{
    void OnNavigatedTo();
    void OnNavigatedFrom() { }
}

public abstract class ViewModelBase : ObservableObject
{
}
