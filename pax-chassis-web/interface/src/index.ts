import type {PaxChassisWeb, InitOutput, initSync} from "./types/pax-chassis-web";

// @ts-ignore
import {ObjectManager} from "./pools/object-manager";
import {
    ANY_CREATE_PATCH,
    FRAME_UPDATE_PATCH,
    IMAGE_LOAD_PATCH, SCROLLER_UPDATE_PATCH,
    SUPPORTED_OBJECTS,
    TEXT_UPDATE_PATCH
} from "./pools/supported-objects";
import {NativeElementPool} from "./classes/native-element-pool";
import {AnyCreatePatch} from "./classes/messages/any-create-patch";
import {TextUpdatePatch} from "./classes/messages/text-update-patch";
import {FrameUpdatePatch} from "./classes/messages/frame-update-patch";
import {ImageLoadPatch} from "./classes/messages/image-load-patch";
import {ScrollerUpdatePatch} from "./classes/messages/scroller-update-patch";
import {setupEventListeners} from "./events/listeners";
import "./styles/pax-web.css";



async function loadWasmModule(extensionless_url: string): Promise<{ chassis: PaxChassisWeb, memory: WebAssembly.Memory }> {
    try {
        const glueCodeModule = await import(`${extensionless_url}.js`) as typeof import("./types/pax-chassis-web");


        const wasmBinary = await fetch(`${extensionless_url}_bg.wasm`);
        const wasmArrayBuffer = await wasmBinary.arrayBuffer();
        let io = glueCodeModule.initSync(wasmArrayBuffer);

        let chassis = glueCodeModule.PaxChassisWeb.new();
        let memory = glueCodeModule.wasm_memory() as WebAssembly.Memory;

        return { chassis, memory };
    } catch (err) {
        throw new Error(`Failed to load WASM module: ${err}`);
    }
}


async function startRenderLoop(wasmUrl: string, mount: Element) {
    try {
        let {chassis, memory} = await loadWasmModule(wasmUrl);
        requestAnimationFrame(renderLoop.bind(renderLoop, chassis, mount, memory));
    } catch (error) {
        console.error("Failed to load or instantiate Wasm module:", error);
    }
}

let initializedChassis = false;
let is_mobile_device = false;

// Init-once globals for garbage collector optimization
let objectManager = new ObjectManager(SUPPORTED_OBJECTS);
let messages : any[];
let nativePool = new NativeElementPool(objectManager);
let textDecoder = new TextDecoder();

function renderLoop (chassis: any, mount: Element, wasm_memory: WebAssembly.Memory) {

    //stats.begin();
    nativePool.sendScrollerValues();
    nativePool.clearCanvases();

    const memorySlice = chassis.tick();
    const memoryBuffer = new Uint8Array(wasm_memory.buffer);

    // Extract the serialized data directly from memory
    const jsonString = textDecoder.decode(memoryBuffer.subarray(memorySlice.ptr(), memorySlice.ptr() + memorySlice.len()));
    messages = JSON.parse(jsonString);

     if(!initializedChassis){
         window.addEventListener('resize', () => {
             let width = window.innerWidth;
             let height = window.innerHeight;
             chassis.send_viewport_update(width, height);
             nativePool.baseOcclusionContext.updateCanvases(width, height);
         });
         setupEventListeners(chassis, mount);
         initializedChassis = true;
     }
     //@ts-ignore
    processMessages(messages, chassis, objectManager);

    // //necessary manual cleanup
    chassis.deallocate(memorySlice);
    //stats.end();
    requestAnimationFrame(renderLoop.bind(renderLoop, chassis, mount, wasm_memory))
}

export function processMessages(messages: any[], chassis: any, objectManager: ObjectManager) {
    messages?.forEach((unwrapped_msg) => {
        if(unwrapped_msg["TextCreate"]) {
            let msg = unwrapped_msg["TextCreate"]
            let patch: AnyCreatePatch = objectManager.getFromPool(ANY_CREATE_PATCH);
            patch.fromPatch(msg);
            nativePool.textCreate(patch);
        }else if (unwrapped_msg["TextUpdate"]){
            let msg = unwrapped_msg["TextUpdate"]
            let patch: TextUpdatePatch = objectManager.getFromPool(TEXT_UPDATE_PATCH, objectManager);
            patch.fromPatch(msg, nativePool.registeredFontFaces);
            nativePool.textUpdate(patch);
        }else if (unwrapped_msg["TextDelete"]) {
            let msg = unwrapped_msg["TextDelete"];
            nativePool.textDelete(msg)
        } else if(unwrapped_msg["FrameCreate"]) {
            let msg = unwrapped_msg["FrameCreate"]
            let patch: AnyCreatePatch = objectManager.getFromPool(ANY_CREATE_PATCH);
            patch.fromPatch(msg);
            nativePool.frameCreate(patch);
        }else if (unwrapped_msg["FrameUpdate"]){
            let msg = unwrapped_msg["FrameUpdate"]
            let patch: FrameUpdatePatch = objectManager.getFromPool(FRAME_UPDATE_PATCH);
            patch.fromPatch(msg);
            nativePool.frameUpdate(patch);
        }else if (unwrapped_msg["FrameDelete"]) {
            let msg = unwrapped_msg["FrameDelete"];
            nativePool.frameDelete(msg["id_chain"])
        }else if (unwrapped_msg["ImageLoad"]){
            let msg = unwrapped_msg["ImageLoad"];
            let patch: ImageLoadPatch = objectManager.getFromPool(IMAGE_LOAD_PATCH);
            patch.fromPatch(msg);
            nativePool.imageLoad(patch, chassis)
        }else if(unwrapped_msg["ScrollerCreate"]) {
            let msg = unwrapped_msg["ScrollerCreate"]
            let patch: AnyCreatePatch = objectManager.getFromPool(ANY_CREATE_PATCH);
            patch.fromPatch(msg);
            nativePool.scrollerCreate(patch, chassis);
        }else if (unwrapped_msg["ScrollerUpdate"]){
            let msg = unwrapped_msg["ScrollerUpdate"]
            let patch : ScrollerUpdatePatch = objectManager.getFromPool(SCROLLER_UPDATE_PATCH);
            patch.fromPatch(msg);
            nativePool.scrollerUpdate(patch);
        }else if (unwrapped_msg["ScrollerDelete"]) {
            let msg = unwrapped_msg["ScrollerDelete"];
            nativePool.scrollerDelete(msg)
        }
    })
}


// Wasm + TS Bootstrapping boilerplate
async function bootstrap(wasmUrl: string, mount: Element) {
    // Start the render loop with the dynamically loaded Wasm module
    startRenderLoop(wasmUrl, mount);
}

export function mount(selector_or_element: string | Element, wasmUrl: string) {

    //Inject CSS
    let link = document.createElement('link')
    link.rel = 'stylesheet'
    link.href = 'pax-chassis-web-interface.css'
    document.head.appendChild(link)

    let mount: Element;
    if (typeof selector_or_element === "string") {
        mount = document.querySelector(selector_or_element) as Element;
    } else {
        mount = selector_or_element;
    }

    // Update to pass wasmUrl to bootstrap function
    if(mount) {
        bootstrap(wasmUrl, mount).then();
    } else {
        console.error("Unable to find mount element");
    }
}