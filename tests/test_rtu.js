import * as qruff from "qruff";

(async () => {
    console.log('before rtu_setup');
    try {
        let rtu = await qruff.rtu_setup('/dev/cu.iPhone-WirelessiAPv2', 9600);
        //let rtu = await qruff.rtu_setup('/dev/usb0', 9600);
    } catch (err) {
        console.log('error is', err);
    }
    console.log('after rtu_setup');
    console.log('rtu is', rtu);
})();
