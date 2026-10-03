pub struct FrozenMigration {
    pub version: i64,
    pub checksum_lf: &'static str,
    pub checksum_crlf: &'static str,
}

impl FrozenMigration {
    pub fn matches(&self, checksum_hex: &str) -> bool {
        checksum_hex == self.checksum_lf || checksum_hex == self.checksum_crlf
    }
}

pub const LEGACY_CHAIN: [FrozenMigration; 46] = [
    FrozenMigration {
        version: 1,
        checksum_lf: "61326cddfe65203cd6369b4f59b21c6c70f1d3f5b1c60216e487b5c85ec823ae898cbfe31546173cad5aa4bbe727929f",
        checksum_crlf: "8c5d80742a9578e23b9a7a6cd30a687efb7105d09aa9eb07f11ee93cf9059f20db89f97c758db1ada3535dab89143242",
    },
    FrozenMigration {
        version: 2,
        checksum_lf: "5ab52bbf7303641ef39ba9c834a61186105923d033a332396caccec7feaa131b431098a7a488af91837bde0fef2d1f65",
        checksum_crlf: "bb81e3da9a16f704fe3b98e93d3cd323d979d2c1bd572a94e8fd1dcf0cd1e9584f1b4aa628c0fad2984e0cb027a2624f",
    },
    FrozenMigration {
        version: 3,
        checksum_lf: "236a8ffd912eefab908b0f025a2fee7d6a5af282155411a7b6ee0f9490123db0c7fc305d359e63b6bfbb0bdf239fe0db",
        checksum_crlf: "4ee0c89957291fb0352edd9188c9ed080e0dfd199f92792fde750ed97475f77ca21a176420f50b95cf48192a7e5bf72d",
    },
    FrozenMigration {
        version: 4,
        checksum_lf: "e7e7c6dbd9957aba7c6df9a117a9d69c0378843d09fa000389a15dc6934df0c71d364cf61a2a35c908c5ee4e2de4a3e5",
        checksum_crlf: "d7166066df2954656d3be5b53baaeb210e5ede296c05ccf889fa9d97531949d88edf6ddc1eef25de85186d34ad4c3598",
    },
    FrozenMigration {
        version: 5,
        checksum_lf: "d043ba9cbbbceacaa27c4fc6520fd6457c89caeeb1be00418a6de85ce87951d5da70ea956b04565a3015bde69f043595",
        checksum_crlf: "4177028c6f1adb3ab6c28b6d4f45ca374b20d30adffae6dbcc2fe94651d12367972fdea2db2c39a6e6dafd1f37b1ae8f",
    },
    FrozenMigration {
        version: 6,
        checksum_lf: "01e9e1754d760782e6c55faa4cea051c41e10a72f7c5941c7384cf37d4f55363642bb3a63adda3bc5bfedcef28100de8",
        checksum_crlf: "08c8aeb2295ec5a57998a8df7c2d748756b526d798bd10b7a490ddba4f08a3389415777948cfe90fa13b90cde9f794d8",
    },
    FrozenMigration {
        version: 7,
        checksum_lf: "3733eb4aaa6a82558ef56e70b991014476c01e0dfac48effc2562c9e0e46b4125a27b2991214ea8eb5518d44c64fc568",
        checksum_crlf: "923de2616d4e1da0f4c7bb68f6a000e6caa1167eced790fa015d049db0dfe37b1c1cf1d438e0f807bcbb251f220a568c",
    },
    FrozenMigration {
        version: 8,
        checksum_lf: "c34c88fdb7abbc15056a8bcc5923857099b29769d0095da20265e88d1bb112f9a5225375a0cfe5944d38aa6159dc90b0",
        checksum_crlf: "54ae57094d0dd2b83d67a7c45f919f03c51f1292162d80762f357a667dda4a17adca92224935e082fe2ccc7f8ab994fd",
    },
    FrozenMigration {
        version: 9,
        checksum_lf: "2bb92be7f158446250f480a09145589500269183d72fe3c0dd64cad904e2a86d47f4f23e1b53cd856e7196904b7f3473",
        checksum_crlf: "fc4e3410a57b230a22f7a6785dd2cdcada8e18a44fb620d3c40ba7f844e66edaf9b5e6fa81d327aa2f8d0457c60b0560",
    },
    FrozenMigration {
        version: 10,
        checksum_lf: "35cc87955a80d105e465a81b9b354f3019213b24f0aaa1e45253fa9c06fe084d4984c656276cd0562292b26c663e3850",
        checksum_crlf: "9d4984ac335510710e4a471f608d9ce5585831c945fab15b04da402c001ca65d6c90159334dac73c384e22155ece07a4",
    },
    FrozenMigration {
        version: 11,
        checksum_lf: "126586c4591557bce66c183f5538a2060d5bdfe85446b31dc02824ea4b4ebd32e43ab4ec7a7eb71b9c84874328c36615",
        checksum_crlf: "7847784405d58498520f85518c4436fb5b86dafe9cd75851048b588dc685282c40ac2625f89d0a2562e2ca0e4431de6c",
    },
    FrozenMigration {
        version: 12,
        checksum_lf: "5bff588e208fb28ed09217e86eb532a167ec75b7cf5871d1a112fd16fd1467604907a476c6d76b5002dde05aee8ae6c8",
        checksum_crlf: "689ffb07425adfeeeaf04c1cb1c66bb133b04e9ea87ce11f00eba3610a5e7721b6ae269858e591527275a73ce0bab5ef",
    },
    FrozenMigration {
        version: 13,
        checksum_lf: "a821a132cbc77751bc0339292f58f2156328f74ce6bbfda7276a02a8906f4d43b7e4b74a8b2c282b331d2e26aefc7326",
        checksum_crlf: "f7eeaf922a026ad739153c06795baf4d4cfc8df1eca7f4d2a41e89487414179dbdcbd7c192af5dc14545c7b863f41e6e",
    },
    FrozenMigration {
        version: 14,
        checksum_lf: "a01d74589873e0baae0abde5fd70ee5402eee36486706b125ef134cbeededa9e18e0419be2b462dea7d87448538e2c83",
        checksum_crlf: "a52a426fc884634ac293542db29f26cb462132a71182b259b41353e8eff669e6834a0177f1113f23818c985fdde8f30c",
    },
    FrozenMigration {
        version: 15,
        checksum_lf: "46136547e070ca0328998736df3979c88e5d3c7cb74e15e46c779e096204723fc5da09bb4528c7082b55b09b88f8a624",
        checksum_crlf: "6eef4135c36cfbda419ad08a3f51af85e6d1f070a14f743263ac0fcf0fdd28971907f3638425ff81b2d4e400b9bc5d23",
    },
    FrozenMigration {
        version: 16,
        checksum_lf: "e0b087668c0608195ab1bb78a8056a15b73c05cfbacda341c568585b80852a33fe3bc819c3668428db42e8eae0f60e95",
        checksum_crlf: "010fde5cf43ca1f43652f2056e34c5dac247b597cac8911aad0188b9471301c2dac71342b590d607c57bd130983a8f6a",
    },
    FrozenMigration {
        version: 17,
        checksum_lf: "784d88ec5617372b99e307e1607ddfba8b88675b1227a24ee04d06a0c3e998e00712cdb94fa1fdd459a22904f4d02016",
        checksum_crlf: "caabb6b81fab6d6855967d708cfe05429f2edc880a05cf00b620bbdc3a9b2762034e15347967a1afd19e57efbf8df749",
    },
    FrozenMigration {
        version: 18,
        checksum_lf: "32df2f93ce1f91da43d36e9327ddb8675ae4fdc5e454903c216fccb2999421afb25324036001b99c3553861679cbec69",
        checksum_crlf: "f4f539fcb88eaa4ab55b51ea73c8490f810820f2cf66adeccbc4fe5fd1d4989329096fffdfb1d5a40ec326e4725246c9",
    },
    FrozenMigration {
        version: 19,
        checksum_lf: "6396ceb3b900a3d4e98f3200b6add0c3af05b807424ee60eb01a9a39f2158e42321cf13ad2b49ecabd2580c88064e5d7",
        checksum_crlf: "00a6ec2abbc30f501b5dbe5392791b0293c62b1aa1dedb81fee5f1cd51d8575d323310737e221c638ca1e0dc5beff7f8",
    },
    FrozenMigration {
        version: 20,
        checksum_lf: "a8dc9e5a2d16a3ba36ea38e48771450f867c43cc33b4b2db19ddfae4d401208cd9656658e2d6e27f8e6e35820ce9e98b",
        checksum_crlf: "6e011f2a09a1c0f639aaa7d244eaaf430ca1a81084c8bd89b1bbf40fd938fc81dd2cc6b944bb14ce08d70b54875772ab",
    },
    FrozenMigration {
        version: 21,
        checksum_lf: "c124edfd40dc3e3569a20f58e5af909347f105778834178dbb6c6a651dd6ad7e745de31a1338841293ed5b5c00483a5d",
        checksum_crlf: "5c74edf1ba862403fdf671e7448b6eecc4593af6893c2a13f5e3ce3a8437aeb2b295b9210d2107ad42b5cb61b3d6b012",
    },
    FrozenMigration {
        version: 22,
        checksum_lf: "fc8cb045dd4837df5b13dd5903a176e4a69c58a29ae7ad244d94ca064975064da795ede7e850c1d46cfe9dce60f7b1b3",
        checksum_crlf: "1e3748f9ed60217157e2622af37fc0408a33a06b9753bd41c8a9b38aa48eb3976ac1d4775f7d14690d79c8345a96fae9",
    },
    FrozenMigration {
        version: 23,
        checksum_lf: "9cb6486fa0a423704a3f01471f265a5bdcafcba0a0f6bcc9c7c0dcd30fcdfee6ee6c288ff953862dec55bd1217994c15",
        checksum_crlf: "67e4eb2466bd4b01147f486502ee2db0f7151b87e29df8c875f384b36698627eee7ce7fe8cfdfb179bedec351e370335",
    },
    FrozenMigration {
        version: 24,
        checksum_lf: "9cf0de874b260300e4d61d56baadacd73cbbb54411683fd9f18cf65420061b1f0d9d169f4e96b3fa576a5a18f4ee179c",
        checksum_crlf: "8e2b5ad7ba6aceef1047d556784dc81035979705f678b396cb8dd42dcf1027b61e0461a4c64d28ff1a715234d98854d4",
    },
    FrozenMigration {
        version: 25,
        checksum_lf: "125c38bd16377d8e1ecf42a7d23b9f769ea73da7b6ae7e052fe6fded3c87dba9321bee99faa8465a4197c06f3bdda304",
        checksum_crlf: "bd2b4c23c6eb19b61d379aeb9ccdad382f14ca1c542d722b56c634ea2509c5be72e2f75956bb4ba0c01e4c901080845c",
    },
    FrozenMigration {
        version: 26,
        checksum_lf: "de77c65379b152733dfbb5089d7aed6f15525b557432c3ede1494df7bf6223b06cfccc47d0efc891b9cfea216e424088",
        checksum_crlf: "7701b1183317b04796ff472432c59152039fccf01649980d01c6a988b16d7ca22daab464fcc373195538a73c98bc1ec6",
    },
    FrozenMigration {
        version: 27,
        checksum_lf: "479f5884f90e809c019a2a6cb472a2eed08a1e3208b2f9450264d39ea5bb7de0994a80f652c40d93ccf77adfab9d5db1",
        checksum_crlf: "0df3376d92c3391348b0dc11352ddafc28c49cc39d99f104f03972138dbb6122f1cb9d61f275dec43a2c3a194c247a41",
    },
    FrozenMigration {
        version: 28,
        checksum_lf: "9793fdfc2c8857cfed37778ff265290afac2097688738834462808a78bebff351e5052abcfd9b62aaaaa42b5f468cd1b",
        checksum_crlf: "125e76e5ba9079e3ab9a4321ad880846b3cb25f5dfe830e4e6e806aaa3c45ca929df73366ef13c6436fd3f9eb68479f2",
    },
    FrozenMigration {
        version: 29,
        checksum_lf: "c5028b521e9c454c887d75916e5bdbf9256fe31b6c7b96234d5e673d47b00b1e6b716d482fb7335d09229b5a4ed4ca64",
        checksum_crlf: "746066f2f7bdccc5016d13740997ffa8127038a1b279d6d42b33f43f94711958e18d2863cfe35c508c5dae4aa1e3fe67",
    },
    FrozenMigration {
        version: 30,
        checksum_lf: "baffe86b153626b28c44f4d99868663f764c6325427e23a9d2947ea2e9711f687d7d72d4a2ef5d09c919f4ff641bcbce",
        checksum_crlf: "262331839154270973dac3743f3b82633b6636a0ef15540d3c3815b32a12da86fbec8c3f4dfe80b07a73f4fc3d64a7f2",
    },
    FrozenMigration {
        version: 31,
        checksum_lf: "779b134fd8e9d25b078915f72228162eb50d5fb93347f89fe4afff884b031f891e92b96959d2831905bdc315f1b4b2a2",
        checksum_crlf: "b8956e28024a04b0030a48aedcccde6175091bddc78f58b65c8b69075f8213e70b0d9a4bce61b37a3a1677dc33fbd83c",
    },
    FrozenMigration {
        version: 32,
        checksum_lf: "d748c279e7a26b83cf94df435c5ceb3debc469903996ac43c8347cb81a5b912ef7fe0f3f8109888b74da612a9cde9950",
        checksum_crlf: "91b01bcbd93e1cf9b71780feb4b1a7f93e88add46ec3b992678d0a192c56b5dab4ac181f90aaebdd5f97a5cf448c4806",
    },
    FrozenMigration {
        version: 33,
        checksum_lf: "a9dfc676cc75d0f9d9185799d5ca1ff96b4b748cae331e23da4d783c38feadaed52e940bbb989559afc57f0763bba1b3",
        checksum_crlf: "09dbba87011b957dabc7d8696395c3933ae77d1066e03641ea3e11887014a447a3ed0d2a599ce9769e85760b5b22474b",
    },
    FrozenMigration {
        version: 34,
        checksum_lf: "0eb8c3158db1117fc8ff68a1e298d6d7c242bbbb8d0c95e734c2aada2b1cd2045b2c238e21491da1b25a89ec2bfdc68d",
        checksum_crlf: "9d5a0593620db0761041183223709fd8e5a63988f8183c3e8f489133b1a4d13c1864005f3af6897d8b7a71abec7e24a2",
    },
    FrozenMigration {
        version: 35,
        checksum_lf: "091c311792a58b164435e320dc2b0d2f372c13b33b273ba736ffd7a96f828ab7c6d1d2a2aad91b58befdf120838d7a38",
        checksum_crlf: "5b6cedbfdd3daac3f89bb2de2640220f7909865962269bd6748b8add1f81f5dd83dd5c09c09fa81cacb1b83c36dd4a88",
    },
    FrozenMigration {
        version: 36,
        checksum_lf: "861494b25be250fc811ce5ffb44c02606ead28e243576a91bbd49fa255f383ade4bde3bb8b4913b729a2a9c6220c491a",
        checksum_crlf: "27c2da70811b5d4bef94b9b1078f74556e1cbe55759509af511c6317e265ee61aeadfe6ae015b275b1a510bd3d7df3b8",
    },
    FrozenMigration {
        version: 37,
        checksum_lf: "f387b45a376f1db985b69f41704e8c753bf9323f522d852593cc2f5f71b72c46fdb3f4c93fff6c7de159cc0415d74e89",
        checksum_crlf: "d5ecf8a75a429fe792c6a9b0efafe78dcf4360929bd757f100daa8c44917b9f8ff425b8fe76ff15a6df321c6032211c9",
    },
    FrozenMigration {
        version: 38,
        checksum_lf: "10ecafc2e149278dd77ff3111909046e1f86f39611ad1370e07c57a6ca6a8a2619ca6cdbed944b6be6cae88fefc93a8c",
        checksum_crlf: "06b456eaea2d045db78f4fdf358ff03dc7233a82a003235e1fd512ef47ce69fdcf960189ca48186dab9a9a22472d9882",
    },
    FrozenMigration {
        version: 39,
        checksum_lf: "e0c4a58b094cd4d04274dec4bff62a99a438e3b93051390bfdeefb810796dbb78d9ddcceba6a39cc18a16cffe95ab7bf",
        checksum_crlf: "d5bff2055445e1b6642a276d96cb40c9faa2b9fafb635220780f3cd87f4f3bc3f2eb25791498e45c76187f57b81af2ac",
    },
    FrozenMigration {
        version: 40,
        checksum_lf: "a3c12514cf6d75b5458739ec93eec3120bfa7770c8230e2cb32743c00eb6d7b71e62ae9c65907479dcf596185c6c95d9",
        checksum_crlf: "1c362d4607428ddb223e5245e83504df2a0fbe3fbd75712fd7112cefd0ee1acb0634946e8d192d559e1438ab5b3a253e",
    },
    FrozenMigration {
        version: 41,
        checksum_lf: "f735326864f447c2ef1b561e3bf3c88a147be526469356aaeef3d56a75c4e1f2921fcaa280724c9adc65693f3b4c168f",
        checksum_crlf: "5c06bfddd0bb95858563278fd0021c64b90789d8e4957a9b63668c4976c9db550e2dbf70ec5f15fc4419da2528a5ce16",
    },
    FrozenMigration {
        version: 42,
        checksum_lf: "2478d48d5df73c0df24236aa0bb44c318d41f179ac4d74384ee7d3bebabee166977121e6594995b5933a01742a15bf0f",
        checksum_crlf: "8db8da2d1cdc6c9fd5f73810b15264df89f56c16a2787facbff5d950957ffd06364ff75537d854749f17c33499384e6c",
    },
    FrozenMigration {
        version: 43,
        checksum_lf: "dacc505dea5c987c672f78f7b4f1f23baf6610e972150b7c6de6aef97aa8509e5c1a48337765773bdac956f7ba5928a5",
        checksum_crlf: "e380d5805d9bb537403b9a69f40323a0f08bfe26eba3883cae426a42aa8b61e02113b27bd74d15ddce50c74d542a0775",
    },
    FrozenMigration {
        version: 44,
        checksum_lf: "2313e9beb0916e0ddd951810fa616df31672b70d504e1a294fddacb45a08e642b8c770a5f51760d97f25f2d24a41d567",
        checksum_crlf: "94c21ff1997622fa2d205948b202e3a0051ba9cd5d4edd5aedba98fea25f24bafd6600bb3ad97b34265c90d25bf842a3",
    },
    FrozenMigration {
        version: 45,
        checksum_lf: "f253e8e30a27073cfcc41c1f56819f368ed28b32a3c0dd3ba53717e28aea06be4a37a8a8f59eb2996c0945e8995a9655",
        checksum_crlf: "56182cbcc10ff4b85c5b9fdb0e71568d9dc604bc5e799f9cd662b8e9fbcf9989af3ae2180259ef535d378cd951c23425",
    },
    FrozenMigration {
        version: 46,
        checksum_lf: "fd2d21220843df8967d3cd5b0a338c5ad1ffc00590545898142eb6faa1c3dded47829aa6fcaf1924844894d11e10a08d",
        checksum_crlf: "c20d83889b50f9d76266cdaec405df7bf5b3335a3a0ba0f5c13872027a9f9ec10fd0c69618666cb8bb0fc82e952c5aed",
    },
];
