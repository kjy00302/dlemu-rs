import json
import sys
import pathlib

path = pathlib.Path(sys.argv[1])
address = sys.argv[2]

with open(path) as f:
    data = json.load(f)

f = open(path.with_suffix(".bulkstream"), 'wb')

for pkt in data:
    usb = pkt['_source']['layers']['usb']
    if usb['usb.device_address'] == address and \
       usb['usb.urb_type'] == "'S'" and \
       usb['usb.transfer_type'] == '0x03' and \
       usb['usb.endpoint_address'] == '0x01':
        data = pkt['_source']['layers']['usb.capdata']
        f.write(bytes.fromhex(data.replace(':', '')))
