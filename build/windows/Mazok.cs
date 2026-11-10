// Мазок — оболочка для Windows (WebView2). Собирается build-exe.ps1 компилятором csc из .NET Framework 4.8.
using System;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.IO.Compression;
using System.Reflection;
using System.Threading.Tasks;
using System.Windows.Forms;
using Microsoft.Win32;
using Microsoft.Web.WebView2.Core;
using Microsoft.Web.WebView2.WinForms;

[assembly: AssemblyTitle("Мазок")]
[assembly: AssemblyProduct("Мазок")]
[assembly: AssemblyDescription("Мазок — приложение для цифрового рисования")]
[assembly: AssemblyVersion("2.0.0.0")]
[assembly: AssemblyFileVersion("2.0.0.0")]

namespace Mazok
{
    static class Program
    {
        public const string Version = "2.0.0";
        public static string Root;

        [STAThread]
        static void Main(string[] args)
        {
            AppDomain.CurrentDomain.AssemblyResolve += Resolve;
            Application.EnableVisualStyles();
            Application.SetCompatibleTextRenderingDefault(false);
            Root = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "Mazok");
            Directory.CreateDirectory(Root);
            if (args.Length > 0 && args[0] == "--uninstall") { Setup.Uninstall(); return; }
            if (args.Length > 1 && args[0] == "--selftest") { SelfTest = args[1]; Start(args); return; }
            if (Setup.MaybeInstall()) return;
            Start(args);
        }

        public static string SelfTest;

        static void Start(string[] args)
        {
            string appDir = Path.Combine(Root, "app-" + Version);
            Extract(appDir);
            Application.Run(new MainForm(appDir, Path.Combine(Root, SelfTest != null ? "selftest-data" : "data")));
        }

        public static Stream Res(string name)
        {
            return Assembly.GetExecutingAssembly().GetManifestResourceStream(name);
        }

        static byte[] ReadAll(Stream s)
        {
            using (MemoryStream m = new MemoryStream()) { s.CopyTo(m); return m.ToArray(); }
        }

        static Assembly Resolve(object sender, ResolveEventArgs e)
        {
            string name = new AssemblyName(e.Name).Name + ".dll";
            using (Stream s = Res(name))
            {
                if (s == null) return null;
                return Assembly.Load(ReadAll(s));
            }
        }

        static void Extract(string dir)
        {
            if (File.Exists(Path.Combine(dir, ".ok"))) return;
            if (Directory.Exists(dir)) Directory.Delete(dir, true);
            Directory.CreateDirectory(dir);
            using (Stream z = Res("app.zip"))
            using (ZipArchive za = new ZipArchive(z, ZipArchiveMode.Read))
            {
                foreach (ZipArchiveEntry en in za.Entries)
                {
                    string path = Path.GetFullPath(Path.Combine(dir, "www", en.FullName));
                    if (!path.StartsWith(Path.GetFullPath(dir), StringComparison.OrdinalIgnoreCase)) continue;
                    if (en.FullName.EndsWith("/")) { Directory.CreateDirectory(path); continue; }
                    Directory.CreateDirectory(Path.GetDirectoryName(path));
                    using (Stream src = en.Open()) using (FileStream dst = File.Create(path)) src.CopyTo(dst);
                }
            }
            using (Stream s = Res("WebView2Loader.dll")) using (FileStream f = File.Create(Path.Combine(dir, "WebView2Loader.dll"))) s.CopyTo(f);
            File.WriteAllText(Path.Combine(dir, ".ok"), Version);
            foreach (string old in Directory.GetDirectories(Root, "app-*"))
                if (!string.Equals(Path.GetFullPath(old), Path.GetFullPath(dir), StringComparison.OrdinalIgnoreCase)) { try { Directory.Delete(old, true); } catch { } }
        }
    }

    class MainForm : Form
    {
        readonly WebView2 web = new WebView2();
        readonly string appDir, dataDir;

        public MainForm(string appDir, string dataDir)
        {
            this.appDir = appDir; this.dataDir = dataDir;
            Text = "Мазок";
            try { Icon = Icon.ExtractAssociatedIcon(Application.ExecutablePath); } catch { }
            BackColor = Color.FromArgb(31, 31, 31);
            StartPosition = FormStartPosition.CenterScreen;
            Rectangle wa = Screen.PrimaryScreen.WorkingArea;
            Size = new Size(Math.Min(1500, wa.Width - 40), Math.Min(950, wa.Height - 40));
            MinimumSize = new Size(640, 480);
            if (wa.Width <= 1600) WindowState = FormWindowState.Maximized;
            web.Dock = DockStyle.Fill;
            web.DefaultBackgroundColor = Color.FromArgb(31, 31, 31);
            Controls.Add(web);
            if (Program.SelfTest != null)
            {
                WindowState = FormWindowState.Normal; StartPosition = FormStartPosition.Manual;
                Location = new Point(-20000, -20000); Size = new Size(1400, 900); ShowInTaskbar = false; Opacity = 0;
            }
            Load += async (s, e) => { await Init(); };
        }

        async Task RunSelfTest()
        {
            string result;
            try
            {
                await Task.Delay(3000);
                result = await web.CoreWebView2.ExecuteScriptAsync(
                    "(function(){try{return JSON.stringify({app:typeof App,desk:UI.desk,gallery:!!document.querySelector('.g-head'),brushes:Brushes.all().length,blends:BLENDS.length,store:Store.persistent,secure:isSecureContext,href:location.href,toast:document.querySelector('#toast').textContent})}catch(e){return 'ERR '+e.message}})()");
            }
            catch (Exception ex) { result = "EXCEPTION " + ex.Message; }
            File.WriteAllText(Program.SelfTest, result);
            Close();
        }

        async Task Init()
        {
            try
            {
                CoreWebView2Environment.SetLoaderDllFolderPath(appDir);
                CoreWebView2Environment env = await CoreWebView2Environment.CreateAsync(null, dataDir, null);
                await web.EnsureCoreWebView2Async(env);
                CoreWebView2 core = web.CoreWebView2;
                core.SetVirtualHostNameToFolderMapping("mazok.app", Path.Combine(appDir, "www"), CoreWebView2HostResourceAccessKind.Allow);
                core.Settings.AreDevToolsEnabled = false;
                core.Settings.IsStatusBarEnabled = false;
                core.Settings.IsZoomControlEnabled = false;
                core.Settings.AreBrowserAcceleratorKeysEnabled = false;
                core.Settings.AreDefaultContextMenusEnabled = false;
                core.DocumentTitleChanged += (s, e) => { Text = core.DocumentTitle; };
                core.NewWindowRequested += (s, e) =>
                {
                    e.Handled = true;
                    if (e.Uri.StartsWith("http")) { try { Process.Start(e.Uri); } catch { } }
                };
                core.PermissionRequested += (s, e) =>
                {
                    if (e.PermissionKind == CoreWebView2PermissionKind.ClipboardRead) e.State = CoreWebView2PermissionState.Allow;
                };
                core.DownloadStarting += (s, e) =>
                {
                    CoreWebView2Deferral def = e.GetDeferral();
                    BeginInvoke(new Action(() =>
                    {
                        try
                        {
                            using (SaveFileDialog dlg = new SaveFileDialog())
                            {
                                string name = Path.GetFileName(e.ResultFilePath);
                                string ext = Path.GetExtension(name);
                                dlg.FileName = name;
                                dlg.Filter = (ext.Length > 1 ? ext.Substring(1).ToUpper() + "|*" + ext + "|" : "") + "Все файлы|*.*";
                                dlg.InitialDirectory = Environment.GetFolderPath(Environment.SpecialFolder.MyPictures);
                                if (dlg.ShowDialog(this) == DialogResult.OK) { e.ResultFilePath = dlg.FileName; e.Handled = true; }
                                else e.Cancel = true;
                            }
                        }
                        finally { def.Complete(); }
                    }));
                };
                if (Program.SelfTest != null) core.NavigationCompleted += async (s, e) => { await RunSelfTest(); };
                core.Navigate("https://mazok.app/index.html");
            }
            catch (Exception ex)
            {
                if (Program.SelfTest != null) { File.WriteAllText(Program.SelfTest, "INIT FAIL " + ex.Message); Close(); return; }
                if (MessageBox.Show(this, "Не удалось запустить встроенный браузерный движок (WebView2):\n" + ex.Message +
                    "\n\nОткрыть страницу загрузки Microsoft Edge WebView2 Runtime?", "Мазок", MessageBoxButtons.YesNo, MessageBoxIcon.Error) == DialogResult.Yes)
                    Process.Start("https://go.microsoft.com/fwlink/p/?LinkId=2124703");
                Close();
            }
        }
    }

    static class Setup
    {
        static string InstallDir { get { return Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "Programs", "Mazok"); } }
        static string InstalledExe { get { return Path.Combine(InstallDir, "Mazok.exe"); } }
        static string StartMenuLnk { get { return Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.Programs), "Мазок.lnk"); } }
        static string DesktopLnk { get { return Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.DesktopDirectory), "Мазок.lnk"); } }
        const string UninstallKey = @"Software\Microsoft\Windows\CurrentVersion\Uninstall\Mazok";

        // true — программа запущена из установленной копии и текущий процесс нужно завершить
        public static bool MaybeInstall()
        {
            string self = Path.GetFullPath(Application.ExecutablePath);
            if (string.Equals(self, Path.GetFullPath(InstalledExe), StringComparison.OrdinalIgnoreCase)) return false;
            bool installed = File.Exists(InstalledExe);
            string flag = Path.Combine(Program.Root, "portable");
            if (!installed && File.Exists(flag)) return false;
            string q = installed
                ? "Мазок уже установлен на этом компьютере. Обновить установленную версию этим файлом?"
                : "Установить Мазок на этот компьютер?\n\nБудут созданы ярлыки в меню «Пуск» и на рабочем столе, программа появится в списке установленных приложений.\n\n«Нет» — просто запустить без установки.";
            DialogResult r = MessageBox.Show(q, "Мазок — установка", MessageBoxButtons.YesNoCancel, MessageBoxIcon.Question);
            if (r == DialogResult.Cancel) return true;
            if (r == DialogResult.No) { if (!installed) File.WriteAllText(flag, "1"); return false; }
            try
            {
                Directory.CreateDirectory(InstallDir);
                File.Copy(self, InstalledExe, true);
                Shortcut(StartMenuLnk);
                Shortcut(DesktopLnk);
                using (RegistryKey k = Registry.CurrentUser.CreateSubKey(UninstallKey))
                {
                    k.SetValue("DisplayName", "Мазок");
                    k.SetValue("DisplayVersion", Program.Version);
                    k.SetValue("Publisher", "Мазок");
                    k.SetValue("DisplayIcon", InstalledExe + ",0");
                    k.SetValue("InstallLocation", InstallDir);
                    k.SetValue("UninstallString", "\"" + InstalledExe + "\" --uninstall");
                    k.SetValue("NoModify", 1, RegistryValueKind.DWord);
                    k.SetValue("NoRepair", 1, RegistryValueKind.DWord);
                    k.SetValue("EstimatedSize", (int)(new FileInfo(InstalledExe).Length / 1024), RegistryValueKind.DWord);
                }
                try { File.Delete(Path.Combine(Program.Root, "portable")); } catch { }
                Process.Start(InstalledExe);
                return true;
            }
            catch (Exception ex)
            {
                MessageBox.Show("Не удалось установить: " + ex.Message + "\nПрограмма запустится без установки.", "Мазок");
                return false;
            }
        }

        static void Shortcut(string path)
        {
            Type t = Type.GetTypeFromProgID("WScript.Shell");
            dynamic sh = Activator.CreateInstance(t);
            dynamic lnk = sh.CreateShortcut(path);
            lnk.TargetPath = InstalledExe;
            lnk.WorkingDirectory = InstallDir;
            lnk.IconLocation = InstalledExe + ",0";
            lnk.Description = "Мазок — цифровое рисование";
            lnk.Save();
        }

        public static void Uninstall()
        {
            if (MessageBox.Show("Удалить программу Мазок с этого компьютера?", "Мазок — удаление", MessageBoxButtons.YesNo, MessageBoxIcon.Question) != DialogResult.Yes) return;
            bool data = MessageBox.Show("Удалить также все рисунки и настройки Мазка?\n\n«Нет» — рисунки останутся и снова появятся после повторной установки.", "Мазок — удаление",
                MessageBoxButtons.YesNo, MessageBoxIcon.Warning, MessageBoxDefaultButton.Button2) == DialogResult.Yes;
            try { File.Delete(StartMenuLnk); } catch { }
            try { File.Delete(DesktopLnk); } catch { }
            try { Registry.CurrentUser.DeleteSubKeyTree(UninstallKey, false); } catch { }
            string targets = "rmdir /s /q \"" + InstallDir + "\"";
            if (data) targets += " & rmdir /s /q \"" + Program.Root + "\"";
            else foreach (string d in Directory.GetDirectories(Program.Root, "app-*")) targets += " & rmdir /s /q \"" + d + "\"";
            MessageBox.Show("Мазок удалён.", "Мазок");
            ProcessStartInfo psi = new ProcessStartInfo("cmd.exe", "/c timeout /t 2 /nobreak >nul & " + targets);
            psi.CreateNoWindow = true; psi.WindowStyle = ProcessWindowStyle.Hidden; psi.UseShellExecute = false;
            Process.Start(psi);
        }
    }
}
