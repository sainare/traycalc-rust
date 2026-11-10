using System;
using System.Diagnostics;
using System.IO;
using System.Reflection;

// This tiny launcher keeps the Rust executable and its runtime DLL inside one
// downloadable EXE. They are unpacked only into the current user's app-data
// folder, never beside the downloaded file.
internal static class Program
{
    [STAThread]
    private static void Main(string[] args)
    {
        string folder = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "TrayCalc", "runtime", "2.0.2");
        Directory.CreateDirectory(folder);

        string app = Path.Combine(folder, "TrayCalcRuntime.exe");
        string runtime = Path.Combine(folder, "libunwind.dll");
        Extract("TrayCalcPayload", app);
        Extract("UnwindPayload", runtime);

        var start = new ProcessStartInfo(app) {
            UseShellExecute = false,
            Arguments = JoinArguments(args),
            WorkingDirectory = folder
        };
        Process.Start(start);
    }

    private static void Extract(string resourceName, string destination)
    {
        Assembly assembly = Assembly.GetExecutingAssembly();
        using (Stream source = assembly.GetManifestResourceStream(resourceName))
        {
            if (source == null) throw new InvalidOperationException("Встроенный файл не найден: " + resourceName);
            if (File.Exists(destination) && new FileInfo(destination).Length == source.Length) return;
            string temporary = destination + ".new";
            using (var target = new FileStream(temporary, FileMode.Create, FileAccess.Write, FileShare.None))
                source.CopyTo(target);
            File.Copy(temporary, destination, true);
            File.Delete(temporary);
        }
    }

    private static string JoinArguments(string[] args)
    {
        var result = new System.Text.StringBuilder();
        foreach (string arg in args) {
            if (result.Length != 0) result.Append(' ');
            result.Append('"').Append(arg.Replace("\\", "\\\\").Replace("\"", "\\\"")).Append('"');
        }
        return result.ToString();
    }
}
