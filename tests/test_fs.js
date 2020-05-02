import * as qruff from "qruff";

let setTimeout = qruff.setTimeout;

async function test_fs() {
    let d = await ru.fs_readall("./tests/test_fs.js");
    console.log('value is', d);
    console.log('len is', d.byteLength);
    console.log(String.fromCharCode.apply(null, new Uint8Array(d, 0, d.byteLength)));
}

console.log('in test_timer');
function test_timer()
{
    //var th, i;
    setTimeout((item)=> {
        console.log('hi i am trigger');
        console.log('item is', item);
    }, 1000);

    setTimeout((item)=> {
        console.log('hi i am trigger');
        console.log('item is', item);
    }, 2000);

}

test_timer();

(async ()=> {
    await test_fs();
})();
