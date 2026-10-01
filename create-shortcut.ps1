# Creates a desktop shortcut for Jad's Brain
$exePath = "D:\Jad's Brain\src-tauri\target\release\jads-brain.exe"
$shortcutPath = [System.IO.Path]::Combine($env:USERPROFILE, "Desktop", "Jad's Brain.lnk")
$iconPath = "D:\Jad's Brain\src-tauri\icons\icon.ico"

$wsh = New-Object -ComObject WScript.Shell
$sc  = $wsh.CreateShortcut($shortcutPath)
$sc.TargetPath       = $exePath
$sc.IconLocation     = "$iconPath,0"
$sc.WorkingDirectory = "D:\Jad's Brain"
$sc.Description      = "Jad's Brain — Personal Second Brain"
$sc.Save()

Write-Host "Raccourci créé : $shortcutPath"
