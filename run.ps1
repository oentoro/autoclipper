# Hentikan proses lama yang mungkin masih berjalan
Stop-Process -Name "autoclipper","llama-server" -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 2

# Jalankan aplikasi
npm run tauri dev
