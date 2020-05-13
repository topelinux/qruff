import * as qruff from "qruff";

let setTimeout = qruff.setTimeout;
let a = setTimeout(() => {
    console.log('Cool Qruff timer 5000ms be triggerd');
}, 1000);

qruff.createCmdGenerator(JSON.stringify(
[
    {id: 'getTemperature', reg_offset: 0x3, reg_len:1, interval: 200},
    {id: 'getHumit', reg_offset: 0x5, reg_len:1, interval: 200},
]
));
