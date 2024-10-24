import {BUTTON_CLASS, BUTTON_TEXT_CONTAINER_CLASS,
    NATIVE_LEAF_CLASS, CHECKBOX_CLASS, RADIO_SET_CLASS,SCROLLER_CONTAINER} from "../utils/constants";
import {AnyCreatePatch} from "./messages/any-create-patch";
import {OcclusionUpdatePatch} from "./messages/occlusion-update-patch";
import snarkdown from 'snarkdown';
import {TextUpdatePatch} from "./messages/text-update-patch";
import {FrameUpdatePatch} from "./messages/frame-update-patch";
import {ScrollerUpdatePatch} from "./messages/scroller-update-patch";
import {ButtonUpdatePatch} from "./messages/button-update-patch";
import {ImageLoadPatch} from "./messages/image-load-patch";
import {ContainerStyle, OcclusionLayerManager} from "./occlusion-context";
import {ObjectManager} from "../pools/object-manager";
import {IMAGE, INPUT, BUTTON, DIV, OCCLUSION_CONTEXT, SELECT} from "../pools/supported-objects";
import {packAffineCoeffsIntoMatrix3DString, readImageToByteBuffer} from "../utils/helpers";
import {ColorGroup, TextStyle, getAlignItems, getJustifyContent, getTextAlign} from "./text";
import type {PaxChassisWeb} from "../types/pax-chassis-web";
import { CheckboxUpdatePatch } from "./messages/checkbox-update-patch";
import { TextboxUpdatePatch } from "./messages/textbox-update-patch";
import { RadioSetUpdatePatch } from "./messages/radio-set-update-patch";
import { DropdownUpdatePatch } from "./messages/dropdown-update-patch";
import { SliderUpdatePatch } from "./messages/slider-update-patch";
import { EventBlockerUpdatePatch } from "./messages/event-blocker-update-patch";
import { NavigationPatch } from "./messages/navigation-patch";
import { NativeImageUpdatePatch } from "./messages/native-image-update-patch";

export class NativeElementPool {
    private canvases: Map<string, HTMLCanvasElement>;
    layers: OcclusionLayerManager;
    private nodesLookup = new Map<number, HTMLElement>();
    private chassis?: PaxChassisWeb;
    private objectManager: ObjectManager;
    private resizeObserver: ResizeObserver;
    registeredFontFaces: Set<string>;

    constructor(objectManager: ObjectManager) {
        this.objectManager = objectManager;
        this.canvases = new Map();
        this.layers = objectManager.getFromPool(OCCLUSION_CONTEXT, objectManager);
        this.registeredFontFaces = new Set<string>();
        this.resizeObserver = new ResizeObserver(entries => {
            let resize_requests = [];
            for (const entry of entries) {
                let node = entry.target as HTMLElement;
                let id = parseInt(node.getAttribute("pax_id")!);
                let width = entry.contentRect.width;
                let height = entry.contentRect.height;
                let message ={
                    "id": id,
                    "width": width,
                    "height": height,
                }
                resize_requests.push(message);
            }
            this.chassis!.interrupt!(JSON.stringify({
                "ChassisResizeRequestCollection": resize_requests,
            }), undefined);
        });
    }

    attach(chassis: PaxChassisWeb, mount: Element){
        this.chassis = chassis;
        this.layers.attach(mount, chassis, this.canvases);
    }

    clearCanvases(): void {
        this.canvases.forEach((canvas, _key) => {
            let dpr = window.devicePixelRatio;
            const context = canvas.getContext('2d');
            if (context) {
                context.clearRect(0, 0, canvas.width, canvas.height);
            }
            if(canvas.width != (canvas.clientWidth * dpr) || canvas.height != (canvas.clientHeight * dpr)){
                canvas.width = (canvas.clientWidth * dpr);
                canvas.height = (canvas.clientHeight * dpr);
                if (context) {
                    context.scale(dpr, dpr);
                }
            }
        });
    }

    occlusionUpdate(patch: OcclusionUpdatePatch) {
        let node: HTMLElement = this.nodesLookup.get(patch.id!)!;
        if (node){
            this.layers.addElement(node, patch.parentFrame, patch.occlusionLayerId!);
            node.style.zIndex = patch.zIndex!.toString();
            const focusableElements = node.querySelectorAll('input, button, select, textarea, a[href]');
            focusableElements.forEach((element, _index) => {
                element.setAttribute('tabindex', (1000000 - patch.zIndex!).toString());
            });
        } else {
            // must be container
            this.layers.updateContainerParent(patch.id!, patch.parentFrame);
        }
    }

    checkboxCreate(patch: AnyCreatePatch) {
        console.assert(patch.id != null);
        console.assert(patch.occlusionLayerId != null);
        
        const checkbox = this.objectManager.getFromPool(INPUT) as HTMLInputElement;
        checkbox.type = "checkbox";
        checkbox.style.margin = "0";
        checkbox.setAttribute("class", CHECKBOX_CLASS);
        checkbox.addEventListener("change", (event) => {
            //Reset the checkbox state (state changes only allowed through engine)
            const is_checked = (event.target as HTMLInputElement).checked;
            checkbox.checked = !is_checked;
            
            let message = {
                "FormCheckboxToggle": {
                    "id": patch.id,
                    "state": is_checked,
                }
            }
            this.chassis!.interrupt(JSON.stringify(message), undefined);
        });

        let checkbox_div: HTMLDivElement = this.objectManager.getFromPool(DIV);
        checkbox_div.appendChild(checkbox);
        checkbox_div.setAttribute("class", NATIVE_LEAF_CLASS)
        checkbox_div.setAttribute("pax_id", String(patch.id));
        if(patch.id != undefined && patch.occlusionLayerId != undefined){
            this.layers.addElement(checkbox_div, patch.parentFrame, patch.occlusionLayerId);
        }
        this.nodesLookup.set(patch.id!, checkbox_div);
    }

    
    checkboxUpdate(patch: CheckboxUpdatePatch) {
        let leaf = this.nodesLookup.get(patch.id!);
        let checkbox = leaf!.firstChild as HTMLInputElement;

        updateCommonProps(leaf!, patch);

        if (patch.checked !== null) {
            checkbox.checked = patch.checked!;
        }

        if (patch.background != null) {
            checkbox.style.background = toCssColor(patch.background);
        }

        if (patch.borderRadius != null) {
            checkbox.style.borderRadius = patch.borderRadius + "px";
        }

        if (patch.outlineWidth !== undefined) {
            checkbox.style.borderWidth = patch.outlineWidth + "px";
        }

        if (patch.outlineColor != null) {
            checkbox.style.borderColor = toCssColor(patch.outlineColor);
        }

        if (patch.backgroundChecked != null) {
            checkbox.style.setProperty("--checked-color", toCssColor(patch.backgroundChecked));
        }
    }

    checkboxDelete(id: number) {
        let oldNode = this.nodesLookup.get(id);
        if (oldNode){
            let parent = oldNode.parentElement;
            parent!.removeChild(oldNode);
            this.nodesLookup.delete(id);
        }
    }

    nativeImageCreate(patch: AnyCreatePatch) {
        console.assert(patch.id != null);
        console.assert(patch.occlusionLayerId != null);
        
        const nativeImage = this.objectManager.getFromPool(IMAGE) as HTMLInputElement;
        nativeImage.style.margin = "0";

        let nativeImage_div: HTMLDivElement = this.objectManager.getFromPool(DIV);
        nativeImage_div.appendChild(nativeImage);
        nativeImage_div.setAttribute("class", NATIVE_LEAF_CLASS)
        nativeImage_div.setAttribute("pax_id", String(patch.id));
        if(patch.id != undefined && patch.occlusionLayerId != undefined){
            this.layers.addElement(nativeImage_div, patch.parentFrame, patch.occlusionLayerId);
        }
        this.nodesLookup.set(patch.id!, nativeImage_div);
    }

    
    nativeImageUpdate(patch: NativeImageUpdatePatch) {
        let leaf = this.nodesLookup.get(patch.id!);
        let nativeImage = leaf!.firstChild as HTMLInputElement;
        updateCommonProps(leaf!, patch);
        if (patch.url != null) {
            nativeImage.setAttribute("src", patch.url);
        }
        if (patch.fit != null) {
            nativeImage.style.objectFit = patch.fit;
        }
    }

    nativeImageDelete(id: number) {
        let oldNode = this.nodesLookup.get(id);
        if (oldNode){
            let parent = oldNode.parentElement;
            parent!.removeChild(oldNode);
            this.nodesLookup.delete(id);
        }
    }

    textboxCreate(patch: AnyCreatePatch) {
        const textbox = this.objectManager.getFromPool(INPUT) as HTMLInputElement;
        textbox.type = "text";
        textbox.style.margin = "0";
        textbox.style.padding = "0";
        textbox.style.paddingInline = "5px 5px";
        textbox.style.paddingBlock = "0";
        textbox.style.borderWidth = "0";
        textbox.addEventListener("input", (_event) => {
            let message = {
                "FormTextboxInput": {
                    "id": patch.id!,
                    "text": textbox.value,
                }
            }
            this.chassis!.interrupt(JSON.stringify(message), undefined);
        });

        textbox.addEventListener("change", (_event) => {
            let message = {
                "FormTextboxChange": {
                    "id": patch.id!,
                    "text": textbox.value,
                }
            }
            this.chassis!.interrupt(JSON.stringify(message), undefined);
        });

        let textboxDiv: HTMLDivElement = this.objectManager.getFromPool(DIV);
        textboxDiv.appendChild(textbox);
        textboxDiv.setAttribute("class", NATIVE_LEAF_CLASS)
        textboxDiv.setAttribute("pax_id", String(patch.id));

        if(patch.id != undefined && patch.occlusionLayerId != undefined){
            this.layers.addElement(textboxDiv, patch.parentFrame, patch.occlusionLayerId);
            this.nodesLookup.set(patch.id!, textboxDiv);
        } else {
            throw new Error("undefined id or occlusionLayer");
        }

    }

    
    textboxUpdate(patch: TextboxUpdatePatch) {
        let leaf = this.nodesLookup.get(patch.id!);
        updateCommonProps(leaf!, patch);
        // set to 10px less to give space for left-padding
        if (patch.size_x != null) {
            (leaf!.firstChild! as HTMLElement).style.width = (patch.size_x - 10) + "px";
        }
        let textbox = leaf!.firstChild as HTMLTextAreaElement;

        applyTextStyle(textbox, textbox, patch.style);

        //We may support styles other than solid in the future; this is a better default than the browser's for now
        textbox.style.borderStyle = "solid";

        if (patch.background != null) {
            textbox.style.background = toCssColor(patch.background);
        }

        if (patch.border_radius != null) {
            textbox.style.borderRadius = patch.border_radius + "px";
        }

        if (patch.stroke_color != null) {
            textbox.style.borderColor = toCssColor(patch.stroke_color);
        }

        if (patch.stroke_width != null) {
            textbox.style.borderWidth = patch.stroke_width + "px";

        }

        // Apply the content
        if (patch.text != null) {

            // Check if the input element is focused — we want to maintain the user's cursor position if so
            if (document.activeElement === textbox) {
                // Get the current selection range
                const selectionStart = textbox.selectionStart || 0;

                // Update the content of the input
                textbox.value = patch.text;

                // Calculate the new cursor position, clamped to the new length of the input value
                const newCursorPosition = Math.min(selectionStart, patch.text.length);

                // Set the cursor position to the beginning of the former selection range
                textbox.setSelectionRange(newCursorPosition, newCursorPosition);
            } else {
                // If the textbox isn't selected, just update its content
                textbox.value = patch.text;
            }


        }
       
        if (patch.focus_on_mount) {
            setTimeout(() => { textbox.focus(); }, 10);
        }
    }

    textboxDelete(id: number) {
        let oldNode = this.nodesLookup.get(id);
        if (oldNode){
            let parent = oldNode.parentElement;
            parent!.removeChild(oldNode);
            this.nodesLookup.delete(id);
        }
    }


    
    radioSetCreate(patch: AnyCreatePatch) {
        let fields = document.createElement('fieldset') as HTMLFieldSetElement;
        fields.style.border = "0";
        fields.style.margin = "0";
        fields.style.padding = "0";
        fields.addEventListener('change', (event) => {
            let target = event.target as HTMLElement | undefined;
            if (target && target.matches("input[type='radio']")) {
                // get the index of the triggered radio button in the fieldset
                let container = target.parentNode as Element;
                let index = Array.from(container!.parentNode!.children).indexOf(container);
                let message = {
                    "FormRadioSetChange": {
                        "id": patch.id!,
                        "selected_id": index,
                    }
                }
                this.chassis!.interrupt(JSON.stringify(message), undefined);
            }
        });

        let radioSetDiv: HTMLDivElement = this.objectManager.getFromPool(DIV);
        radioSetDiv.setAttribute("class", NATIVE_LEAF_CLASS)
        radioSetDiv.setAttribute("pax_id", String(patch.id));
        radioSetDiv.appendChild(fields);

        if(patch.id != undefined && patch.occlusionLayerId != undefined){
            this.layers.addElement(radioSetDiv, patch.parentFrame, patch.occlusionLayerId);
            this.nodesLookup.set(patch.id!, radioSetDiv);
        } else {
            throw new Error("undefined id or occlusionLayer");
        }

    }

    
    radioSetUpdate(patch: RadioSetUpdatePatch) {
        let leaf = this.nodesLookup.get(patch.id!);
        updateCommonProps(leaf!, patch);
        if (patch.style != null) {
            applyTextStyle(leaf!, leaf!, patch.style);
        }

        let fields = leaf!.firstChild as HTMLFieldSetElement;
        if (patch.options != null) {
            fields!.innerHTML = "";
            patch.options.forEach((optionText, _index) => {
                let div = document.createElement('div') as HTMLDivElement;
                div.style.alignItems = "center";
                div.style.display = "flex";
                div.style.marginBottom = "3px";
                const option = document.createElement('input') as HTMLInputElement;
                option.type = "radio";
                option.name = `radio-${patch.id}`;
                option.value = optionText.toString();
                option.setAttribute("class", RADIO_SET_CLASS);
                div.appendChild(option);
                const label = document.createElement('label') as HTMLLabelElement;
                label.innerHTML = optionText.toString();
                div.appendChild(label);
                fields.appendChild(div);
            });
        }

        if (patch.selected_id != null) {
            let radio = fields.children[patch.selected_id].firstChild as HTMLInputElement;
            if (radio.checked == false) {
                radio.checked = true;
            }
        }

        if (patch.background != null) {
           fields.style.setProperty("--background-color", toCssColor(patch.background));
        }

        if (patch.backgroundChecked != null) {
           fields.style.setProperty("--selected-color", toCssColor(patch.backgroundChecked));
        }

        if (patch.outlineWidth != null) {
            fields.style.setProperty("--border-width", patch.outlineWidth + "px");
        }

        if (patch.outlineColor != null) {
            fields.style.setProperty("--border-color",  toCssColor(patch.outlineColor));
        }
    }

    radioSetDelete(id: number) {
        let oldNode = this.nodesLookup.get(id);
        if (oldNode){
            let parent = oldNode.parentElement;
            parent!.removeChild(oldNode);
            this.nodesLookup.delete(id);
        }
    }

    
    sliderCreate(patch: AnyCreatePatch) {
        const slider = this.objectManager.getFromPool(INPUT) as HTMLInputElement;
        slider.type = "range";
        slider.style.padding = "0px";
        slider.style.margin = "0px";
        slider.style.appearance = "none";
        slider.style.display = "block";
        slider.addEventListener("input", (_event) => {
            let message = {
                "FormSliderChange": {
                    "id": patch.id!,
                    "value": parseFloat(slider.value),
                }
            }
            this.chassis!.interrupt(JSON.stringify(message), undefined);
        });

        let sliderDiv: HTMLDivElement = this.objectManager.getFromPool(DIV);
        sliderDiv.appendChild(slider);
        sliderDiv.setAttribute("class", NATIVE_LEAF_CLASS)
        sliderDiv.style.overflow = "visible";
        sliderDiv.setAttribute("pax_id", String(patch.id));

        if(patch.id != undefined && patch.occlusionLayerId != undefined){
            this.layers.addElement(sliderDiv, patch.parentFrame, patch.occlusionLayerId);
            this.nodesLookup.set(patch.id!, sliderDiv);
        } else {
            throw new Error("undefined id or occlusionLayer");
        }

    }

    
    sliderUpdate(patch: SliderUpdatePatch) {
        let leaf = this.nodesLookup.get(patch.id!);
        updateCommonProps(leaf!, patch);
        let slider = leaf!.firstChild as HTMLInputElement;

        if (patch.step != null && patch.step.toString() != slider.step) {
            slider.step = patch.step.toString();
        }
        if (patch.min != null && patch.min.toString() != slider.min) {
            slider.min = patch.min.toString();
        }
        if (patch.max != null && patch.max.toString() != slider.max) {
            slider.max = patch.max.toString();
        }
        if (patch.value != null && patch.value.toString() != slider.value) {
            slider.value = patch.value.toString();
        }

        if (patch.accent != null) {
            let color =  toCssColor(patch.accent);   
            slider.style.accentColor = color;
        }

        if (patch.background != null) {
            let color =  toCssColor(patch.background);   
            slider.style.backgroundColor = color;
        }

        if (patch.borderRadius != null) {
            slider.style.borderRadius = patch.borderRadius + "px";
        }
    }

    sliderDelete(id: number) {
        let oldNode = this.nodesLookup.get(id);
        if (oldNode){
            let parent = oldNode.parentElement;
            parent!.removeChild(oldNode);
            this.nodesLookup.delete(id);
        }
    }

    dropdownCreate(patch: AnyCreatePatch) {
        const dropdown = this.objectManager.getFromPool(SELECT) as HTMLSelectElement;
        dropdown.addEventListener("change", (event) => {
            let message = {
                "FormDropdownChange": {
                    "id": patch.id!,
                    "selected_id": (event.target! as any).selectedIndex,
                }
            }
            this.chassis!.interrupt(JSON.stringify(message), undefined);
        });


        let textboxDiv: HTMLDivElement = this.objectManager.getFromPool(DIV);
        textboxDiv.appendChild(dropdown);
        textboxDiv.setAttribute("class", NATIVE_LEAF_CLASS)
        textboxDiv.setAttribute("pax_id", String(patch.id));

        if(patch.id != undefined && patch.occlusionLayerId != undefined){
            this.layers.addElement(textboxDiv, patch.parentFrame, patch.occlusionLayerId);
            this.nodesLookup.set(patch.id!, textboxDiv);
        } else {
            throw new Error("undefined id or occlusionLayer");
        }

    }

    
    dropdownUpdate(patch: DropdownUpdatePatch) {
        let leaf = this.nodesLookup.get(patch.id!);
        updateCommonProps(leaf!, patch);
        let dropdown = leaf!.firstChild as HTMLSelectElement;
        applyTextStyle(dropdown, dropdown, patch.style);
        dropdown.style.borderStyle = "solid";

        if (patch.background != null) {
            dropdown.style.backgroundColor = toCssColor(patch.background);
        }
        if (patch.stroke_color != null) {
            dropdown.style.borderColor = toCssColor(patch.stroke_color);
        }
        if (patch.stroke_width != null) {
            dropdown.style.borderWidth = patch.stroke_width + "px";
        }

        if (patch.borderRadius != null) {
            dropdown.style.borderRadius = patch.borderRadius + "px";
        }

        // Apply the content
        if (patch.options != null) {
            // Iterate over the options array and create option elements

            //clear children
            dropdown.innerHTML = "";

            patch.options.forEach((optionText, index) => {
                const option = document.createElement('option') as HTMLOptionElement;
                option.value = index.toString();
                option.textContent = optionText;
                dropdown.appendChild(option);
            });
        }

        if (patch.selected_id != null && dropdown.options.selectedIndex != patch.selected_id) {
            dropdown.options.selectedIndex = patch.selected_id;
        }
    }

    dropdownDelete(id: number) {
        let oldNode = this.nodesLookup.get(id);
        if (oldNode){
            let parent = oldNode.parentElement;
            parent!.removeChild(oldNode);
            this.nodesLookup.delete(id);
        }
    }

    buttonCreate(patch: AnyCreatePatch) {
        console.assert(patch.id != null);
        console.assert(patch.occlusionLayerId != null);
        
        const button = this.objectManager.getFromPool(BUTTON) as HTMLButtonElement;
        const textContainer = this.objectManager.getFromPool(DIV) as HTMLDivElement;
        const textChild = this.objectManager.getFromPool(DIV) as HTMLDivElement;
        button.setAttribute("class", BUTTON_CLASS);
        textContainer.setAttribute("class", BUTTON_TEXT_CONTAINER_CLASS);
        textChild.style.margin = "0";
        button.addEventListener("click", (_event) => {
            let message = {
                "FormButtonClick": {
                    "id": patch.id!,
                }
            }
            this.chassis!.interrupt(JSON.stringify(message), undefined);
        });

        let buttonDiv: HTMLDivElement = this.objectManager.getFromPool(DIV);
        textContainer.appendChild(textChild);
        button.appendChild(textContainer);
        buttonDiv.appendChild(button);
        buttonDiv.setAttribute("class", NATIVE_LEAF_CLASS)
        buttonDiv.setAttribute("pax_id", String(patch.id));
        if(patch.id != undefined && patch.occlusionLayerId != undefined){
            this.layers.addElement(buttonDiv, patch.parentFrame, patch.occlusionLayerId);
            this.nodesLookup.set(patch.id!, buttonDiv);
        } else {
            throw new Error("undefined id or occlusionLayer");
        }
    }

    
    buttonUpdate(patch: ButtonUpdatePatch) {
        let leaf = this.nodesLookup.get(patch.id!);
        updateCommonProps(leaf!, patch);
        console.assert(leaf !== undefined);
        let button = leaf!.firstChild as HTMLElement;
        let textContainer = button!.firstChild as HTMLElement;
        let textChild = textContainer.firstChild as HTMLElement;


        // Apply the content
        if (patch.content != null) {
            textChild.innerHTML = snarkdown(patch.content);
        }
        // if not applied, rendering moves button down
        if (textChild.innerHTML.length == 0) {
            textChild.innerHTML = " ";
        }

        if (patch.color != null) {
            button.style.background = toCssColor(patch.color);
        }

        if (patch.hoverColor != null) {
            let color = toCssColor(patch.hoverColor);
            button.style.setProperty("--hover-color", color);
        }

        if (patch.borderRadius != null) {
            button.style.borderRadius = patch.borderRadius + "px";
        }

        if (patch.outlineStrokeColor != null) {
            button.style.borderColor = toCssColor(patch.outlineStrokeColor);
        }

        if (patch.outlineStrokeWidth != null) {
            button.style.borderWidth = patch.outlineStrokeWidth + "px";
        }
        
        applyTextStyle(textContainer, textChild, patch.style);
    }

    buttonDelete(id: number) {
        let oldNode = this.nodesLookup.get(id);
        if (oldNode){
            let parent = oldNode.parentElement;
            parent!.removeChild(oldNode);
            this.nodesLookup.delete(id);
        }
    }

    textCreate(patch: AnyCreatePatch) {
        console.assert(patch.id != null);
        console.assert(patch.occlusionLayerId != null);

        let textDiv: HTMLDivElement = this.objectManager.getFromPool(DIV);
        let textChild: HTMLDivElement = this.objectManager.getFromPool(DIV);
        textChild.addEventListener("input", (_event) => {
            let message = {
              "TextInput": {
                "id": patch.id!,
                "text": sanitizeContentEditableString(textChild.innerHTML),
              }
            };

            this.chassis!.interrupt(JSON.stringify(message), undefined);
        });
        textDiv.appendChild(textChild);
        textDiv.setAttribute("class", NATIVE_LEAF_CLASS)
        textDiv.setAttribute("pax_id", String(patch.id));

        if(patch.id != undefined && patch.occlusionLayerId != undefined){
            this.layers.addElement(textDiv, patch.parentFrame, patch.occlusionLayerId);
            this.nodesLookup.set(patch.id!, textDiv);
        } else {
            throw new Error("undefined id or occlusionLayer");
        }
    }

    textUpdate(patch: TextUpdatePatch) {
        let leaf = this.nodesLookup.get(patch.id!) as HTMLElement;
        let textChild = leaf!.firstChild as HTMLElement;
        // should be start listening to this elements size and
        // send interrupts to the engine, or not?
        let start_listening = false;

        // Handle size_x and size_y
        if (patch.size_x != null) {

            // if size_x = -1.0, the engine wants to know
            // this elements size from the chassi.
            if (patch.size_x == -1.0) {
                start_listening = true;
            } else {
                leaf!.style.width = patch.size_x + "px";
            }
        }
        if (patch.size_y != null) {
            if (patch.size_y == -1.0) {
                start_listening = true;
            } else {
                leaf!.style.height = patch.size_y + "px";
            }
        }

        if (start_listening) {
            this.resizeObserver.observe(leaf);
        }

        // Handle transform
        if (patch.transform != null) {
            leaf!.style.transform = packAffineCoeffsIntoMatrix3DString(patch.transform);
        }

        if (patch.editable != null) {
            textChild.setAttribute("contenteditable", patch.editable.toString());
            const selection = window.getSelection();
            selection!.removeAllRanges();
            if (patch.editable == true) {
                textChild.style.outline = "none";

                 // Select all text in the editable div
                const range = document.createRange();
                range.selectNodeContents(textChild);
                selection!.addRange(range);

                setTimeout(() => {
                  textChild.focus();
                }, 1);
                // Focus on the editable div
            }
        }

        if (patch.selectable != null) {
            textChild.style.userSelect = patch.selectable ? "auto" : "none";
        }

        applyTextStyle(leaf, textChild, patch.style);

        // Apply the content
        if (patch.content != null) {
            if (sanitizeContentEditableString(textChild.innerHTML) != patch.content) {
                textChild.innerHTML = snarkdown(patch.content);
            }
            // Apply the link styles if they exist
            if (patch.style_link != null) {
                let linkStyle = patch.style_link;
                const links = textChild.querySelectorAll('a');
                links.forEach((link: HTMLElement) => {
                    if (linkStyle.font) {
                        linkStyle.font.applyFontToDiv(link);
                    }
                    if (linkStyle.fill) {
                        let newValue = "";
                        if(linkStyle.fill.Rgba != null) {
                            let p = linkStyle.fill.Rgba;
                            newValue = `rgba(${p[0]! * 255.0},${p[1]! * 255.0},${p[2]! * 255.0},${p[3]!})`; //note that alpha channel expects [0.0, 1.0] in CSS
                        } else {
                            console.warn("Unsupported Color Format");
                        }
                        link.style.color = newValue;
                    }

                    if (linkStyle.align_horizontal) {
                        leaf.style.display = "flex";
                        leaf.style.justifyContent = getJustifyContent(linkStyle.align_horizontal);
                    }
                    if (linkStyle.font_size) {
                        textChild.style.fontSize = linkStyle.font_size + "px";
                    }
                    if (linkStyle.align_vertical) {
                        leaf.style.alignItems = getAlignItems(linkStyle.align_vertical);
                    }
                    if (linkStyle.align_multiline) {
                        textChild.style.textAlign = getTextAlign(linkStyle.align_multiline);
                    }
                    //force underlining for now since we don't currently offer an API that offers sane (underlined) defaults.
                    link.style.textDecoration = 'underline';
                });
            }
        }
    }

    textDelete(id: number) {
        let oldNode = this.nodesLookup.get(id);
        this.resizeObserver.unobserve(oldNode!);
        if (oldNode){
            let parent = oldNode.parentElement;
            parent!.removeChild(oldNode);
            this.nodesLookup.delete(id);
        }
    }

    frameCreate(patch: AnyCreatePatch) {
        console.assert(patch.id != null);
        this.layers.addContainer(patch.id!, patch.parentFrame);
    }

    frameUpdate(patch: FrameUpdatePatch) {
        console.assert(patch.id != null);

        let styles: Partial<ContainerStyle> = {};
         if (patch.sizeX != null) {
             styles.width = patch.sizeX;
         }
         if (patch.sizeY != null) {
             styles.height = patch.sizeY;
         }
         if (patch.transform != null) {
            styles.transform = patch.transform;
         }
         if (patch.clipContent != null) {
             styles.clipContent = patch.clipContent;
         }
        
        this.layers.updateContainer(patch.id!, styles);
    }

    frameDelete(id: number) {
        this.layers.removeContainer(id);
    }

    scrollerCreate(patch: AnyCreatePatch){
        console.assert(patch.id != null);
        console.assert(patch.occlusionLayerId != null);

        let scrollerDiv: HTMLDivElement = this.objectManager.getFromPool(DIV);
        let scroller: HTMLDivElement = this.objectManager.getFromPool(DIV);
        scroller.style.pointerEvents = "none";
        scrollerDiv.addEventListener("scroll", (_event) => {
            let message = {
                "Scrollbar": {
                    "id": patch.id!,
                    "scroll_x": scrollerDiv.scrollLeft,
                    "scroll_y": scrollerDiv.scrollTop
                }
            }
            this.chassis!.interrupt(JSON.stringify(message), undefined);
        });

        scrollerDiv.appendChild(scroller);
        scrollerDiv.setAttribute("class", NATIVE_LEAF_CLASS + " " + SCROLLER_CONTAINER)
        scrollerDiv.setAttribute("pax_id", String(patch.id));


        if(patch.id != undefined && patch.occlusionLayerId != undefined){
            this.layers.addElement(scrollerDiv, patch.parentFrame, patch.occlusionLayerId);
            this.nodesLookup.set(patch.id!, scrollerDiv);
        } else {
            throw new Error("undefined id or occlusionLayer");
        }
    }

    scrollerUpdate(patch: ScrollerUpdatePatch){
        let leaf = this.nodesLookup.get(patch.id!);
        // Ordering sometimes result in updates being sent after deletes.
        // could fix this ordering, but simply "skipping" this works for now.
        if (leaf == undefined) {
            return;
        }
        let scroller_inner = leaf.firstChild as HTMLElement;

        // Handle size_x and size_y
        if (patch.sizeX != null) {
            leaf.style.width = patch.sizeX + "px";
        }
        if (patch.sizeY != null) {
            leaf.style.height = patch.sizeY + "px";
        }

        if (patch.scrollX != null) {
            leaf.scrollLeft = patch.scrollX;
        }
        if (patch.scrollY != null) {
            leaf.scrollTop = patch.scrollY;
        }

        // Handle transform
        if (patch.transform != null) {
            leaf.style.transform = packAffineCoeffsIntoMatrix3DString(patch.transform);
        }

        if (patch.sizeInnerPaneX != null) {
            if (patch.sizeInnerPaneX! <= parseFloat(leaf.style.width)) {
                leaf.style.overflowX = "hidden";
            } else {
                leaf.style.overflowX = "auto";
            }
            scroller_inner.style.width = patch.sizeInnerPaneX + "px";
        }
        if (patch.sizeInnerPaneY != null) {
            if (patch.sizeInnerPaneY! <= parseFloat(leaf.style.height)) {
                leaf.style.overflowY = "hidden";
            } else {
                leaf.style.overflowY = "auto";
            }
            scroller_inner.style.height = patch.sizeInnerPaneY + "px";
        }
    }

    scrollerDelete(id: number){
        let oldNode = this.nodesLookup.get(id);
        if (oldNode == undefined) {
            throw new Error("tried to delete non-existent scroller");
        }
        let parent = oldNode.parentElement!;
        parent.removeChild(oldNode);
        this.nodesLookup.delete(id);
    }

    eventBlockerCreate(patch: AnyCreatePatch){
        console.assert(patch.id != null);
        console.assert(patch.occlusionLayerId != null);

        let eventBlockerDiv: HTMLDivElement = this.objectManager.getFromPool(DIV);
        // let eventBlocker: HTMLDivElement = this.objectManager.getFromPool(DIV);
        eventBlockerDiv.setAttribute("class", NATIVE_LEAF_CLASS)
        eventBlockerDiv.setAttribute("pax_id", String(patch.id));


        if(patch.id != undefined && patch.occlusionLayerId != undefined){
            this.layers.addElement(eventBlockerDiv, patch.parentFrame, patch.occlusionLayerId);
            this.nodesLookup.set(patch.id!, eventBlockerDiv);
        } else {
            throw new Error("undefined id or occlusionLayer");
        }


    }

    eventBlockerUpdate(patch: EventBlockerUpdatePatch){
        let leaf = this.nodesLookup.get(patch.id!);
        if (leaf == undefined) {
            throw new Error("tried to update non-existent event blocker");
        }
        // Handle size_x and size_y
        if (patch.sizeX != null) {
            leaf.style.width = patch.sizeX + "px";
        }
        if (patch.sizeY != null) {
            leaf.style.height = patch.sizeY + "px";
        }
        // Handle transform
        if (patch.transform != null) {
            leaf.style.transform = packAffineCoeffsIntoMatrix3DString(patch.transform);
        }
    }

    eventBlockerDelete(id: number){
        let oldNode = this.nodesLookup.get(id);
        if (oldNode == undefined) {
            throw new Error("tried to delete non-existent event blocker");
        }
        let parent = oldNode.parentElement!;
        parent.removeChild(oldNode);
        this.nodesLookup.delete(id);
    }



    async imageLoad(patch: ImageLoadPatch, chassis: PaxChassisWeb) {

        if (chassis.image_loaded(patch.path ?? "")) {
            return
        }
        //Check the full path of our index.js; use the prefix of this path also for our image assets
        function getBasePath() {
            const baseURI = document.baseURI;
            const url = new URL(baseURI);
            return url.pathname.substring(0, url.pathname.lastIndexOf('/') + 1);
        }
    
        const BASE_PATH = getBasePath();

        let path = (BASE_PATH + patch.path!).replace("//", "/");
        let image_data = await readImageToByteBuffer(path!)
        let message = {
            "Image": {
                "Data": {
                    "id": patch.id!,
                    "path": patch.path!,
                    "width": image_data.width,
                    "height": image_data.height,
                }
            }
        }
        chassis.interrupt(JSON.stringify(message), image_data.pixels);
    }

    navigate(patch: NavigationPatch) {
        let name: string;
        switch (patch.target) {
            case "current":
                name = "_self";
                break;
            case "new":
                name = "_blank";
                break;
            default:
                console.error("no valid url target!");
                name = "_self";
        }
        window.open(patch.url, name);
    }
}

function toCssColor(color: ColorGroup): string {
    if (color.Rgba != null) {
        let p = color.Rgba;
        return `rgba(${p[0] * 255},${p[1] * 255},${p[2] * 255},${p[3]})`; //Note that alpha channel expects [0.0, 1.0] in CSS
    } else {
        throw new TypeError("Unsupported Color Format");
    }        
}

function applyTextStyle(textContainer: HTMLElement, textElem: HTMLElement, style: TextStyle | undefined) {
    
    // Apply TextStyle from patch.style
    if (style) {
        if (style.font) {
            style.font.applyFontToDiv(textContainer);
        }
        if (style.fill) {
            textElem.style.color = toCssColor(style.fill);
        }
        if (style.font_size) {
            textElem.style.fontSize = style.font_size + "px";
        }
        if (style.underline != null) {
            textElem.style.textDecoration = style.underline ? 'underline' : 'none';
        }
        if (style.align_horizontal) {
            textContainer.style.display = "flex";
            textContainer.style.justifyContent = getJustifyContent(style.align_horizontal);
        }
        if (style.align_vertical) {
            textContainer.style.alignItems = getAlignItems(style.align_vertical);
        }
        if (style.align_multiline) {
            textElem.style.textAlign = getTextAlign(style.align_multiline);
        }
    }
}


// why all the replaces?:
// see: https://stackoverflow.com/questions/13762863/contenteditable-field-to-maintain-newlines-upon-database-entry
function sanitizeContentEditableString(string: string): string {
    return (string
        .replace(/<br\s*\/*>/ig, '\n') 
        .replace(/(<(p|div))/ig, '\n$1') 
        .replace(/(<([^>]+)>)/ig, "")?? '');
}

function updateCommonProps(leaf: HTMLElement, patch: any) {
    let elem = leaf!.firstChild as any;
    // Handle size_x and size_y
    if (patch.size_x != null) {
        elem!.style.width = patch.size_x + "px";
    }
    if (patch.size_y != null) {
        elem!.style.height = patch.size_y + "px";
    }
    // Handle transform
    if (patch.transform != null) {
        leaf!.style.transform = packAffineCoeffsIntoMatrix3DString(patch.transform);
    }
}
