@echo off
echo Starting SSH tunnels to server10.doodkin.com...
echo   API:    localhost:3000  -> server10:13000 -> socat SSL -> :443
echo   Worker: localhost:8080  -> server10:18080 -> socat SSL -> :8443
echo.
echo Press Ctrl+C to stop tunnels.
ssh -N -o ServerAliveInterval=30 -o ServerAliveCountMax=3 -R 0.0.0.0:13000:localhost:3000 -R 0.0.0.0:18080:localhost:8080 server10.doodkin.com
