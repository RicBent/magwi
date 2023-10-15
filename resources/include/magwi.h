#pragma once

#ifndef __INTELLISENSE__

    #if __GNUC__ < 11
        #error "magwi requires GCC >= 11.0"
    #endif

    #ifndef __mw_symbol_safe_filename
        #error "__mw_symbol_safe_filename must be defined"
    #endif

#endif

#ifndef __ASSEMBLER__

    #define __mw_hook_label_impl2(type, arg, file, line, counter) \
        __attribute__((used, __symver__("__mw_hook_" #type "$" #arg "$" #file "$" #line "$" #counter "@0")))

    #define __mw_hook_label_impl(type, arg, file, line, counter) \
        __mw_hook_label_impl2(type, arg, file, line, counter)

    #define __mw_hook_label(type, arg) \
        __mw_hook_label_impl(type, arg, __mw_symbol_safe_filename, __LINE__, __COUNTER__)

    #define __mw_section_impl2(type, arg, file, line, counter) \
        __attribute__((used, section(".__mw_hook_" #type "$" #arg "$" #file "$" #line "$" #counter)))

    #define __mw_section_impl(type, arg, file, line, counter) \
        __mw_section_impl2(type, arg, file, line, counter)

    #define __mw_section(type, arg) \
        __mw_section_impl(type, arg, __mw_symbol_safe_filename, __LINE__, __COUNTER__)

    #define mw_replace(address) __mw_section(replace, address)

    #define mw_loader_code \
        __attribute__((section(".mw_loader_text"), optimize("Os")))

    #ifdef __cplusplus
        #define __mw_extern extern "C"
    #else
        #define __mw_extern extern
    #endif

    __mw_extern char __mw_text_start;
    __mw_extern char __mw_text_end;

#else

    #define __mw_hook_label_impl2(type, arg, file, line, counter) .global __mw_hook_##type##$##arg##$##file##$##line##$##counter; __mw_hook_##type##$##arg##$##file##$##line##$##counter:

    #define __mw_hook_label_impl(type, arg, file, line, counter) \
        __mw_hook_label_impl2(type, arg, file, line, counter)

    #define __mw_hook_label(type, arg) \
        __mw_hook_label_impl(type, arg, __mw_symbol_safe_filename, __LINE__, __COUNTER__)

    #define __mw_section_impl2(type, arg, file, line, counter) \
        .pushsection .__mw_hook_##type##$##arg##$##file##$##line##$##counter

    #define __mw_section_impl(type, arg, file, line, counter) \
        __mw_section_impl2(type, arg, file, line, counter)

    #define __mw_section(type, arg) \
        __mw_section_impl(type, arg, __mw_symbol_safe_filename, __LINE__, __COUNTER__)

    #define mw_replace(address) __mw_section(replace, address)
    #define mw_replace_end .popsection

    #define mw_loader_section .mw_loader_text

#endif

#define mw_b(address) __mw_hook_label(b, address)
#define mw_beq(address) __mw_hook_label(beq, address)
#define mw_bne(address) __mw_hook_label(bne, address)
#define mw_bcs(address) __mw_hook_label(bcs, address)
#define mw_bcc(address) __mw_hook_label(bcc, address)
#define mw_bmi(address) __mw_hook_label(bmi, address)
#define mw_bpl(address) __mw_hook_label(bpl, address)
#define mw_bvs(address) __mw_hook_label(bvs, address)
#define mw_bvc(address) __mw_hook_label(bvc, address)
#define mw_bhi(address) __mw_hook_label(bhi, address)
#define mw_bls(address) __mw_hook_label(bls, address)
#define mw_bge(address) __mw_hook_label(bge, address)
#define mw_blt(address) __mw_hook_label(blt, address)
#define mw_bgt(address) __mw_hook_label(bgt, address)
#define mw_ble(address) __mw_hook_label(ble, address)

#define mw_bl(address) __mw_hook_label(bl, address)
#define mw_bleq(address) __mw_hook_label(bleq, address)
#define mw_blne(address) __mw_hook_label(blne, address)
#define mw_blcs(address) __mw_hook_label(blcs, address)
#define mw_blcc(address) __mw_hook_label(blcc, address)
#define mw_blmi(address) __mw_hook_label(blmi, address)
#define mw_blpl(address) __mw_hook_label(blpl, address)
#define mw_blvs(address) __mw_hook_label(blvs, address)
#define mw_blvc(address) __mw_hook_label(blvc, address)
#define mw_blhi(address) __mw_hook_label(blhi, address)
#define mw_blls(address) __mw_hook_label(blls, address)
#define mw_blge(address) __mw_hook_label(blge, address)
#define mw_bllt(address) __mw_hook_label(bllt, address)
#define mw_blgt(address) __mw_hook_label(blgt, address)
#define mw_blle(address) __mw_hook_label(blle, address)

#define mw_pre(address) __mw_hook_label(pre, address)
#define mw_post(address) __mw_hook_label(post, address)

#define mw_symptr(address) __mw_hook_label(symptr, address)
