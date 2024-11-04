# dlemu-rs

Renders display frame buffer from DisplayLink's bulk transfer stream.

![bad apple demo w/debugdraw&info](https://github.com/user-attachments/assets/aa981cc9-dc7b-46c4-afe0-890a6d192e9f)

## Usage

    Usage: dlemu-rs [OPTIONS] <FILE>

    Arguments:
      <FILE>

    Options:
      -d, --debugdraw
      -i, --info
      -p, --pause
      -f, --fps <FPS>                [default: 60]
          --buffersize <BUFFERSIZE>  [default: 10]
      -h, --help                     Print help
      -V, --version                  Print version

## Keyboard controls

 - Space: play/pause
 - Period: frame by frame skip
 - Q: quit program
 - I: toggle info/register view
 - D: toggle debug draw

## Extracting bulk transfer stream from pcap

From Wireshark, export packet dissections as json. Then use `extractbulk.py <FILENAME> <ADDRESS>` to extract bulk transfer stream.
