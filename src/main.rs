use inkwell::{
    context::Context,
    passes::PassManager,
    targets::{CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine},
    OptimizationLevel,
};

fn main() {
    Target::initialize_all(&InitializationConfig {
        base: true,
        info: true,
        asm_printer: true,
        asm_parser: true,
        ..Default::default()
    });

    let context = Context::create();
    let module = context.create_module("avx_test");
    let builder = context.create_builder();

    let i64t = context.i64_type();
    let i64t_vt = i64t.vec_type(16);

    let return_type = context.struct_type(&[i64t.into(), i64t.into(), i64t.into(), i64t_vt.into()], false);

    let fn_type = return_type.fn_type(&[i64t.into(), i64t.into(), i64t.into(), i64t_vt.into()], false);
    let function = module.add_function("foo", fn_type, None);
    let basic_block = context.append_basic_block(function, "entry");

    builder.position_at_end(basic_block);

    let v0 = function.get_nth_param(3).unwrap().into_vector_value();

    let mut result = vec![];

    for i in 0..16 {
        let e = builder
            .build_extract_element(v0, i64t.const_int(i, false), "vector_element")
            .into_int_value();
        result.push(e);
    }

    // Process values in result so the generated code is not a simply shuffle
    {
        result[0] = builder.build_int_add(result[0], i64t.const_int(42, false).into(), "add");
        result[1] = builder.build_int_mul(result[1], i64t.const_int(3, false).into(), "mul");
        result[2] = builder.build_int_sub(result[2], i64t.const_int(100, false).into(), "add");
        result[3] = builder.build_int_sub(result[3], i64t.const_int(123, false).into(), "mul");
        result[4] = builder.build_int_add(result[4], i64t.const_int(1024, false).into(), "add");
        result[5] = builder.build_int_mul(result[5], i64t.const_int(7, false).into(), "mul");
        // result[9] = builder.build_int_mul(result[9], i64t.const_int(11, false).into(), "add");
        // result[15] = builder.build_int_mul(result[15], i64t.const_int(3, false).into(), "mul");
    }

    let mut vret = i64t_vt.get_undef();
    for i in 0..16 {
        vret = builder.build_insert_element(
            vret,
            result[i],
            i64t.const_int(i as u64, false),
            "insert_element",
        );
    }

    builder.build_aggregate_return(&[
        builder.build_int_add(function.get_nth_param(0).unwrap().into_int_value(), i64t.const_int(0xFF, false), "param0").into(),
        i64t.const_int(0xFFFF, false).into(),
        function.get_nth_param(2).unwrap(),
        vret.into(),
    ]);

    assert!(function.verify(true));

    let pass: PassManager<_> = PassManager::create(());
    pass.add_promote_memory_to_register_pass();
    pass.add_instruction_combining_pass();
    pass.add_reassociate_pass();
    pass.add_gvn_pass();
    pass.add_cfg_simplification_pass();
    pass.run_on(&module);

    let bitcode = module.write_bitcode_to_memory();
    std::fs::write("avx_test.bc", bitcode.as_slice()).unwrap();

    let triple = TargetMachine::get_default_triple();
    let target = Target::from_triple(&triple).unwrap();
    let tm = target
        .create_target_machine(
            &triple,
            "generic",
            "+avx2,+sse4.2",
            OptimizationLevel::Default,
            RelocMode::PIC,
            CodeModel::Default,
        )
        .unwrap();
    let buf = tm.write_to_memory_buffer(&module, FileType::Object).unwrap();
    std::fs::write("avx_test.o", buf.as_slice()).unwrap();
}
