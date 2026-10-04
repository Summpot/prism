using System;
using System.IO;
using System.IO.Pipes;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Avalonia;
using Avalonia.Threading;
using Prism.Services;

namespace Prism;

internal sealed class Program
{
    private const string PipeName = "PrismDesktopIpcPipe";
    private const string MutexName = "PrismDesktopSingleInstanceMutex";

    [STAThread]
    public static void Main(string[] args)
    {
        // Handle restart handover: wait for previous instance to exit if PRISM_RESTART_PID is present
        if (int.TryParse(Environment.GetEnvironmentVariable("PRISM_RESTART_PID"), out int restartPid))
        {
            try
            {
                using var prevProc = System.Diagnostics.Process.GetProcessById(restartPid);
                prevProc.WaitForExit(5000);
            }
            catch
            {
                // Process already terminated
            }
        }

        // A named mutex and a named pipe are the single-instance path on Windows, Linux, and macOS.
        Mutex? mutex = null;
        bool isFirstInstance = false;
        for (int i = 0; i < 10; i++)
        {
            try
            {
                mutex = new Mutex(true, MutexName, out isFirstInstance);
                if (isFirstInstance)
                {
                    break;
                }
                mutex.Dispose();
                mutex = null;
            }
            catch
            {
                mutex?.Dispose();
                mutex = null;
            }
            Thread.Sleep(100);
        }

        if (!isFirstInstance || mutex == null)
        {
            // Another instance is already running; forward args and exit immediately
            try
            {
                using var pipeClient = new NamedPipeClientStream(".", PipeName, PipeDirection.Out);
                pipeClient.Connect(1500);
                using var writer = new StreamWriter(pipeClient, Encoding.UTF8);
                string payload = args.Length > 0 ? string.Join("\n", args) : "activate";
                writer.WriteLine(payload);
                writer.Flush();
            }
            catch (Exception ex)
            {
                Console.WriteLine($"[WARN] Failed to forward args to primary instance: {ex.Message}");
            }
            return;
        }

        using (mutex)
        {
            using var cts = new CancellationTokenSource();
            _ = Task.Run(() => RunIpcServerAsync(cts.Token));

            DesktopService.RegisterPrismProtocol();

            try
            {
                BuildAvaloniaApp()
                    .StartWithClassicDesktopLifetime(args);
            }
            catch (Exception ex)
            {
                Console.WriteLine($"[FATAL] Unhandled application exception: {ex}");
            }
            finally
            {
                cts.Cancel();
            }
        }
    }

    private static async Task RunIpcServerAsync(CancellationToken token)
    {
        while (!token.IsCancellationRequested)
        {
            try
            {
                using var server = new NamedPipeServerStream(PipeName, PipeDirection.In, 1, PipeTransmissionMode.Byte, PipeOptions.Asynchronous);
                await server.WaitForConnectionAsync(token);
                using var reader = new StreamReader(server, Encoding.UTF8);
                string? line = await reader.ReadToEndAsync(token);
                if (!string.IsNullOrWhiteSpace(line))
                {
                    var lines = line.Split('\n', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries);
                    Dispatcher.UIThread.Post(() => App.HandleIncomingArgs(lines));
                }
            }
            catch (OperationCanceledException)
            {
                break;
            }
            catch (Exception ex)
            {
                Console.WriteLine($"[WARN] IPC server error: {ex.Message}");
            }
        }
    }

    public static AppBuilder BuildAvaloniaApp()
    {
        return AppBuilder.Configure<App>()
            .UsePlatformDetect()
            .LogToTrace();
    }
}
