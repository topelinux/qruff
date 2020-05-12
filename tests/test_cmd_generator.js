import * as qruff from "qruff";

qruff.createCmdGenerator(JSON.stringify(
[
    {id: 'getTemperature', reg_offset: 0x3, reg_len:1, interval: 200},
    {id: 'getHumit', reg_offset: 0x5, reg_len:1, interval: 200},
]
));
