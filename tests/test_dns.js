import * as qruff from "qruff";

qruff.getAddrInfo('baidu.com').then((name) => {
    let addrStr = String.fromCharCode.apply(null, new Uint8Array(name, 0, name.byteLength));

    console.log(addrStr);
    try {
        let addr = JSON.parse(addrStr);
        for (let v in addr) {
            let info = addr[v];
            for (let v in info) {
                console.log(`${v}: `, info[v]);
            }
        }
    } catch(err) {
        console.log('err is', err);
    }
});
