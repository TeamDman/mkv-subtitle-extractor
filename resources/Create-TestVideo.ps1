ffmpeg -f lavfi `
    -i color=c=black:s=1280x720:r=30 `
    -t 10 `
    -c:v libx264 `
    -pix_fmt yuv420p `
    black10s.mkv

ffmpeg -i black10s.mkv `
    -i test.srt `
    -i test.ass `
    -c copy `
    -map 0:v `
    -map 1:0 `
    -map 2:0 `
    output_with_subs.mkv
